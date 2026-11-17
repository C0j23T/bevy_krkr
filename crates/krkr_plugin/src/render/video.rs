use std::{panic::AssertUnwindSafe, sync::Mutex};

#[cfg(target_os = "linux")]
use std::os::fd::FromRawFd;

use bevy::render::{
    render_resource::{
        Extent3d, TexelCopyBufferLayout, Texture, TextureDescriptor, TextureDimension,
        TextureFormat, TextureUsages,
    },
    renderer::{RenderAdapter, RenderDevice, RenderQueue},
};

#[cfg(target_os = "android")]
use wgpu_external_frame::ahardware_buffer::{DeviceRequirements, HardwareBufferImporter};
#[cfg(target_os = "android")]
use wgpu_external_frame::ahardware_buffer::{HardwareBufferFrame, HardwareBufferLease};

#[cfg(target_os = "ios")]
use wgpu_external_frame::io_surface::{Ycbcr420IoSurfaceFrame, Ycbcr420Plane};
#[cfg(any(target_os = "linux", target_os = "ios"))]
use wgpu_external_frame::{YcbcrEncoding, YcbcrMatrix, YcbcrRange};

#[cfg(target_os = "linux")]
use wgpu_external_frame::dma_buf::{DmaBufFormat, DmaBufFrame, DmaBufImporter, DmaBufPlane};

static VIDEO_GPU: Mutex<Option<VideoGpu>> = Mutex::new(None);

struct VideoGpu {
    device: RenderDevice,
    queue: RenderQueue,
    #[cfg(target_os = "linux")]
    importer: Option<DmaBufImporter>,
    #[cfg(target_os = "android")]
    importer: Option<HardwareBufferImporter>,
    #[cfg(target_os = "android")]
    converter: Option<VideoYcbcrConverter>,
    #[cfg(target_os = "android")]
    import_format: Option<&'static str>,
    #[cfg(any(target_os = "linux", target_os = "ios"))]
    converter: Option<VideoYcbcrConverter>,
}

#[cfg(target_os = "android")]
pub(crate) struct AndroidVideoOutput {
    reader: std::sync::Arc<AndroidReader>,
}

#[cfg(target_os = "android")]
pub(crate) struct AndroidVideoFrame {
    buffer: HardwareBufferFrame,
    crop: ndk::media::image_reader::CropRect,
}

#[cfg(target_os = "ios")]
#[link(name = "CoreVideo", kind = "framework")]
unsafe extern "C" {
    fn CVPixelBufferGetIOSurface(buffer: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
}

#[cfg(target_os = "android")]
struct AndroidImageLease {
    _image: ndk::media::image_reader::Image,
    _reader: std::sync::Arc<AndroidReader>,
}

#[cfg(target_os = "android")]
struct AndroidReader(ndk::media::image_reader::ImageReader);

// Reader access is serialized by the decoder; leases only keep it alive. NDK deletion is thread safe.
#[cfg(target_os = "android")]
unsafe impl Send for AndroidReader {}
#[cfg(target_os = "android")]
unsafe impl Sync for AndroidReader {}

#[cfg(target_os = "android")]
unsafe extern "C" {
    fn av_mediacodec_release_buffer(buffer: *mut std::ffi::c_void, render: i32) -> i32;
}

// The lease owns the image exclusively; deletion runs after its GPU reads finish.
#[cfg(target_os = "android")]
unsafe impl Send for AndroidImageLease {}

#[cfg(target_os = "android")]
impl std::fmt::Debug for AndroidImageLease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AndroidImageLease")
    }
}

#[cfg(target_os = "android")]
impl HardwareBufferLease for AndroidImageLease {
    fn presented(&mut self) {}

    fn release(self: Box<Self>) {
        drop(self);
    }
}

#[cfg(target_os = "android")]
impl AndroidVideoOutput {
    pub(crate) fn new(
        width: u32,
        height: u32,
    ) -> Result<(Self, *mut ffmpeg_next::ffi::AVBufferRef), String> {
        use ndk::{
            hardware_buffer::HardwareBufferUsage,
            media::image_reader::{ImageFormat, ImageReader},
        };
        if width == 0 || height == 0 || width > i32::MAX as u32 || height > i32::MAX as u32 {
            return Err("invalid MediaCodec output dimensions".into());
        }
        let usage = HardwareBufferUsage::GPU_SAMPLED_IMAGE | HardwareBufferUsage::CPU_READ_NEVER;
        let mut reader = ImageReader::new_with_usage(
            width as i32,
            height as i32,
            ImageFormat::YUV_420_888,
            usage,
            8,
        )
        .map_err(|e| e.to_string())?;
        // Each lease retains its Image and reader, so removed buffers need no separate cache eviction.
        reader
            .set_buffer_removed_listener(Box::new(|_, _| {}))
            .map_err(|e| e.to_string())?;
        let window = reader.window().map_err(|e| e.to_string())?;
        let device_type =
            unsafe { ffmpeg_next::ffi::av_hwdevice_find_type_by_name(c"mediacodec".as_ptr()) };
        if device_type == ffmpeg_next::ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_NONE {
            return Err("FFmpeg MediaCodec hardware device unavailable".into());
        }
        let device = unsafe { ffmpeg_next::ffi::av_hwdevice_ctx_alloc(device_type) };
        if device.is_null() {
            return Err("failed to allocate FFmpeg MediaCodec device".into());
        }
        let context = unsafe { &mut *(*device).data.cast::<ffmpeg_next::ffi::AVHWDeviceContext>() };
        let media_codec = context
            .hwctx
            .cast::<ffmpeg_next::ffi::AVMediaCodecDeviceContext>();
        if media_codec.is_null() {
            let mut device = device;
            unsafe {
                ffmpeg_next::ffi::av_buffer_unref(&mut device);
            }
            return Err("FFmpeg MediaCodec device context is unavailable".into());
        }
        unsafe {
            (*media_codec).native_window = window.ptr().as_ptr().cast();
            if ffmpeg_next::ffi::av_hwdevice_ctx_init(device) < 0 {
                let mut device = device;
                ffmpeg_next::ffi::av_buffer_unref(&mut device);
                return Err("failed to initialize FFmpeg MediaCodec output surface".into());
            }
        }
        Ok((
            Self {
                reader: std::sync::Arc::new(AndroidReader(reader)),
            },
            device,
        ))
    }

    pub(crate) fn acquire(
        &self,
        native: &ffmpeg_next::frame::Video,
    ) -> Result<AndroidVideoFrame, String> {
        use ndk::media::image_reader::AcquireResult;
        use std::os::fd::OwnedFd;

        if native.format() != ffmpeg_next::format::Pixel::MEDIACODEC {
            return Err("MediaCodec returned a software frame".into());
        }
        let buffer = unsafe { (*native.as_ptr()).data[3].cast::<std::ffi::c_void>() };
        if buffer.is_null() {
            return Err("MediaCodec frame has no output buffer".into());
        }
        let status = unsafe { av_mediacodec_release_buffer(buffer, 1) };
        if status < 0 {
            return Err("failed to release MediaCodec frame to ImageReader".into());
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(100);
        let (image, fence): (ndk::media::image_reader::Image, Option<OwnedFd>) = loop {
            match unsafe { self.reader.0.acquire_next_image_async() }.map_err(|e| e.to_string())? {
                AcquireResult::Image((image, fence)) => break (image, fence),
                AcquireResult::NoBufferAvailable if std::time::Instant::now() < deadline => {
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
                _ => return Err("ImageReader has no available decoded frame".into()),
            }
        };
        let buffer = image.hardware_buffer().map_err(|e| e.to_string())?;
        let crop = image.crop_rect().map_err(|e| e.to_string())?;
        Ok(AndroidVideoFrame {
            buffer: HardwareBufferFrame::new(&buffer, fence).with_lease(Box::new(
                AndroidImageLease {
                    _image: image,
                    _reader: self.reader.clone(),
                },
            )),
            crop,
        })
    }

    pub(crate) fn discard_pending(&self) -> Result<(), String> {
        use ndk::media::image_reader::AcquireResult;

        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(100);
        loop {
            match unsafe { self.reader.0.acquire_latest_image_async() }
                .map_err(|e| e.to_string())?
            {
                AcquireResult::Image((image, _fence)) => drop(image),
                AcquireResult::NoBufferAvailable => return Ok(()),
                AcquireResult::MaxImagesAcquired if std::time::Instant::now() < deadline => {
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
                AcquireResult::MaxImagesAcquired => {
                    return Err("ImageReader images are still in use during seek".into());
                }
            }
        }
    }
}

#[cfg(target_os = "android")]
pub(crate) fn android_import_available() -> bool {
    VIDEO_GPU
        .lock()
        .ok()
        .and_then(|gpu| gpu.as_ref().map(|gpu| gpu.importer.is_some()))
        .unwrap_or(false)
}

#[cfg(target_os = "android")]
fn import_android(
    frame: Option<AndroidVideoFrame>,
    width: u32,
    height: u32,
) -> Option<(Texture, bool)> {
    use wgpu_external_frame::ahardware_buffer::ImportedHardwareBuffer;

    let frame = frame?;
    let description = frame.buffer.buffer().describe();
    let crop = frame.crop;
    if width == 0
        || height == 0
        || crop.left < 0
        || crop.top < 0
        || i64::from(crop.right) - i64::from(crop.left) != i64::from(width)
        || i64::from(crop.bottom) - i64::from(crop.top) != i64::from(height)
        || i64::from(crop.right) > i64::from(description.width)
        || i64::from(crop.bottom) > i64::from(description.height)
    {
        bevy::log::warn!(
            "video path: invalid AHardwareBuffer crop {crop:?}, buffer={}x{}, video={width}x{height}",
            description.width,
            description.height,
        );
        return None;
    }
    let origin = [crop.left as u32, crop.top as u32];
    let force_opaque = frame.buffer.force_opaque();
    let mut guard = VIDEO_GPU.lock().ok()?;
    let gpu = guard.as_mut()?;
    let _submission = super::surface_sync::lock();
    let importer = gpu.importer.as_mut()?;
    let imported =
        match std::panic::catch_unwind(AssertUnwindSafe(|| importer.import(frame.buffer))) {
            Ok(Ok(imported)) => imported,
            Ok(Err(error)) => {
                bevy::log::warn!("video path: AHardwareBuffer import failed: {error:?}");
                return None;
            }
            Err(_) => {
                bevy::log::warn!("video path: AHardwareBuffer importer panicked");
                return None;
            }
        };
    let format = match &imported {
        ImportedHardwareBuffer::Rgba(_) => "RGBA alias",
        ImportedHardwareBuffer::Ycbcr420(_) => "YCbCr420 + GPU conversion",
    };
    let (texture, force_opaque) = match imported {
        ImportedHardwareBuffer::Rgba(texture) => {
            let texture =
                if origin == [0, 0] && texture.width() == width && texture.height() == height {
                    texture
                } else {
                    let output = gpu.device.wgpu_device().create_texture(&TextureDescriptor {
                        label: Some("Kirikiri video cropped RGBA"),
                        size: Extent3d {
                            width,
                            height,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: TextureDimension::D2,
                        format: texture.format(),
                        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
                        view_formats: &[],
                    });
                    let mut encoder = gpu.device.create_command_encoder(&Default::default());
                    encoder.copy_texture_to_texture(
                        wgpu::TexelCopyTextureInfo {
                            origin: wgpu::Origin3d {
                                x: origin[0],
                                y: origin[1],
                                z: 0,
                            },
                            ..texture.as_image_copy()
                        },
                        output.as_image_copy(),
                        output.size(),
                    );
                    gpu.queue.submit([encoder.finish()]);
                    output
                };
            (texture, force_opaque)
        }
        ImportedHardwareBuffer::Ycbcr420(planes) => {
            let converter = gpu
                .converter
                .get_or_insert_with(|| VideoYcbcrConverter::new(gpu.device.wgpu_device()));
            (
                converter.convert(
                    gpu.device.wgpu_device(),
                    &gpu.queue,
                    &planes.luma,
                    &planes.chroma,
                    planes.encoding,
                    width,
                    height,
                    origin,
                ),
                true,
            )
        }
    };
    if texture.width() != width || texture.height() != height {
        return None;
    }
    if gpu.import_format != Some(format) {
        bevy::log::info!(
            "video path: AHardwareBuffer -> wgpu Vulkan import succeeded ({format}); buffer={}x{} format={:?} crop={crop:?}; CPU pixel upload bypassed",
            description.width,
            description.height,
            description.format,
        );
        gpu.import_format = Some(format);
    }
    Some((texture.into(), force_opaque))
}

#[cfg(any(target_os = "linux", target_os = "android", target_os = "ios", test))]
struct VideoYcbcrConverter {
    pipeline: wgpu::ComputePipeline,
    parameters: wgpu::Buffer,
}

#[cfg(any(target_os = "linux", target_os = "android", target_os = "ios", test))]
fn video_ycbcr_rows(encoding: wgpu_external_frame::YcbcrEncoding) -> [[f32; 4]; 3] {
    use wgpu_external_frame::{YcbcrMatrix, YcbcrRange};

    let (kr, kb) = match encoding.matrix {
        YcbcrMatrix::Bt601 => (0.299_f32, 0.114_f32),
        YcbcrMatrix::Bt709 => (0.2126_f32, 0.0722_f32),
        YcbcrMatrix::Bt2020 => (0.2627_f32, 0.0593_f32),
    };
    let kg = 1.0 - kr - kb;
    let (ys, yb, cs) = match encoding.range {
        YcbcrRange::Full => (1.0, 0.0, 1.0),
        YcbcrRange::Video => (255.0 / 219.0, -16.0 / 219.0, 255.0 / 224.0),
    };
    let rv = 2.0 * (1.0 - kr) * cs;
    let bu = 2.0 * (1.0 - kb) * cs;
    let gu = -2.0 * kb * (1.0 - kb) / kg * cs;
    let gv = -2.0 * kr * (1.0 - kr) / kg * cs;
    let cb = 128.0 / 255.0;
    [
        [ys, 0.0, rv, yb - rv * cb],
        [ys, gu, gv, yb - (gu + gv) * cb],
        [ys, bu, 0.0, yb - bu * cb],
    ]
}

#[cfg(any(target_os = "linux", target_os = "android", target_os = "ios", test))]
impl VideoYcbcrConverter {
    fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Kirikiri video YCbCr conversion"),
            source: wgpu::ShaderSource::Wgsl(
                r#"
struct Conversion {
    rows: array<vec4<f32>, 3>,
    origin: vec4<u32>,
}

@group(0) @binding(0) var luma: texture_2d<f32>;
@group(0) @binding(1) var chroma: texture_2d<f32>;
@group(0) @binding(2) var output: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(3) var<uniform> conversion: Conversion;

fn sample_chroma(position: vec2<u32>) -> vec2<f32> {
    let size = textureDimensions(chroma);
    let coordinate = (vec2<f32>(position) + vec2<f32>(0.5)) * 0.5 - vec2<f32>(0.5);
    let base = vec2<i32>(floor(coordinate));
    let fraction = fract(coordinate);
    let maximum = vec2<i32>(size) - vec2<i32>(1);
    let top_left = textureLoad(chroma, clamp(base, vec2<i32>(0), maximum), 0).rg;
    let top_right = textureLoad(chroma, clamp(base + vec2<i32>(1, 0), vec2<i32>(0), maximum), 0).rg;
    let bottom_left = textureLoad(chroma, clamp(base + vec2<i32>(0, 1), vec2<i32>(0), maximum), 0).rg;
    let bottom_right = textureLoad(chroma, clamp(base + vec2<i32>(1, 1), vec2<i32>(0), maximum), 0).rg;
    return mix(mix(top_left, top_right, fraction.x), mix(bottom_left, bottom_right, fraction.x), fraction.y);
}

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = textureDimensions(output);
    if any(id.xy >= size) {
        return;
    }
    let source = id.xy + conversion.origin.xy;
    let y = textureLoad(luma, vec2<i32>(source), 0).r;
    let uv = sample_chroma(source);
    let sample = vec4<f32>(y, uv, 1.0);
    let rgb = vec3<f32>(
        dot(conversion.rows[0], sample),
        dot(conversion.rows[1], sample),
        dot(conversion.rows[2], sample)
    );
    textureStore(output, vec2<i32>(id.xy), vec4<f32>(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0));
}
"#
                .into(),
            ),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Kirikiri video YCbCr conversion"),
            layout: None,
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        let parameters = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Kirikiri video YCbCr matrix"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            pipeline,
            parameters,
        }
    }

    fn convert(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        luma: &wgpu::TextureView,
        chroma: &wgpu::TextureView,
        encoding: wgpu_external_frame::YcbcrEncoding,
        width: u32,
        height: u32,
        origin: [u32; 2],
    ) -> wgpu::Texture {
        let output = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Kirikiri video converted RGBA"),
            size: Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8Unorm,
            usage: TextureUsages::TEXTURE_BINDING
                | TextureUsages::STORAGE_BINDING
                | TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = output.create_view(&Default::default());
        let bindings = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Kirikiri video YCbCr planes"),
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(luma),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(chroma),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: self.parameters.as_entire_binding(),
                },
            ],
        });
        queue.write_buffer(
            &self.parameters,
            0,
            bytemuck::cast_slice(&video_ycbcr_rows(encoding)),
        );
        queue.write_buffer(
            &self.parameters,
            48,
            bytemuck::cast_slice(&[origin[0], origin[1], 0, 0]),
        );
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Kirikiri video YCbCr conversion"),
        });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Kirikiri video YCbCr conversion"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bindings, &[]);
            pass.dispatch_workgroups(width.div_ceil(8), height.div_ceil(8), 1);
        }
        queue.submit([encoder.finish()]);
        output
    }
}

#[cfg(any(target_os = "linux", target_os = "ios"))]
fn video_ycbcr_encoding(frame: &ffmpeg_next::frame::Video) -> YcbcrEncoding {
    use ffmpeg_next::util::color::{range::Range, space::Space};

    let matrix = match frame.color_space() {
        Space::BT709 => YcbcrMatrix::Bt709,
        Space::BT2020NCL | Space::BT2020CL => YcbcrMatrix::Bt2020,
        _ => YcbcrMatrix::Bt601,
    };
    let range = match frame.color_range() {
        Range::JPEG => YcbcrRange::Full,
        _ => YcbcrRange::Video,
    };
    YcbcrEncoding { matrix, range }
}

#[cfg(target_os = "android")]
struct VideoDeviceRequirements(DeviceRequirements);

// The Vulkan feature chain has no thread affinity; boxed storage remains fixed until device creation finishes.
#[cfg(target_os = "android")]
unsafe impl Send for VideoDeviceRequirements {}

#[cfg(target_os = "android")]
fn android_device_supports_import(device: &RenderDevice) -> bool {
    let Some(device) = (unsafe { device.wgpu_device().as_hal::<wgpu::hal::api::Vulkan>() }) else {
        return false;
    };
    wgpu_external_frame::ahardware_buffer::DEVICE_EXTENSIONS
        .iter()
        .all(|extension| device.enabled_device_extensions().contains(extension))
}

#[cfg(target_os = "android")]
pub(crate) fn configure_vulkan_import(app: &mut bevy::prelude::App) {
    use bevy::render::renderer::raw_vulkan_init::RawVulkanInitSettings;

    let mut settings = app
        .world()
        .get_resource::<RawVulkanInitSettings>()
        .cloned()
        .unwrap_or_default();
    unsafe {
        let requirements = Mutex::new(Vec::<Box<VideoDeviceRequirements>>::new());
        settings.add_create_device_callback(move |args, adapter, _| {
            if let Ok(value) = DeviceRequirements::new(adapter) {
                let mut storage = requirements.lock().unwrap();
                storage.push(Box::new(VideoDeviceRequirements(value)));
                // The callback owns this allocation until device creation has finished.
                let requirements = &mut storage.last_mut().unwrap().0 as *mut DeviceRequirements;
                (&mut *requirements).add_to(args);
            }
        });
    }
    app.insert_resource(settings);
}

pub(crate) fn probe(device: &RenderDevice, adapter: &RenderAdapter) -> &'static str {
    let backend = adapter.get_info().backend;
    #[cfg(target_os = "linux")]
    {
        if backend != wgpu::Backend::Vulkan {
            return "CPU upload: Linux video import requires the Vulkan backend";
        }
        if !linux_device_supports_import(device) {
            return "CPU upload: Vulkan DMA-BUF device extensions are unavailable";
        }
        return "Vulkan DMA-BUF import available; FFmpeg export format is validated per frame";
    }
    #[cfg(target_os = "ios")]
    {
        return if backend != wgpu::Backend::Metal {
            "CPU upload: iOS video import requires the Metal backend"
        } else {
            let _ = device;
            "Metal IOSurface import available; VideoToolbox format is validated per frame"
        };
    }
    #[cfg(target_os = "android")]
    {
        return if backend != wgpu::Backend::Vulkan {
            "CPU upload: Android video import requires the Vulkan backend"
        } else if !android_device_supports_import(device) {
            "CPU upload: add KrkrVideoPlugin before DefaultPlugins to enable AHardwareBuffer device extensions"
        } else {
            "Vulkan AHardwareBuffer import available; MediaCodec output is validated per frame"
        };
    }
    #[cfg(not(any(target_os = "linux", target_os = "ios", target_os = "android")))]
    {
        let _ = (device, backend);
        "CPU upload: no video frame import backend for this platform"
    }
}

pub(crate) fn initialize(device: RenderDevice, queue: RenderQueue, adapter: Option<RenderAdapter>) {
    #[cfg(target_os = "linux")]
    let importer = adapter
        .filter(|_| linux_device_supports_import(&device))
        .map(|adapter| DmaBufImporter::new(device.wgpu_device(), &queue, &adapter));
    #[cfg(target_os = "linux")]
    let converter = importer
        .as_ref()
        .map(|_| VideoYcbcrConverter::new(device.wgpu_device()));
    #[cfg(target_os = "android")]
    let importer = adapter
        .filter(|_| android_device_supports_import(&device))
        .and_then(|adapter| {
            let hal_adapter = unsafe { adapter.as_hal::<wgpu::hal::api::Vulkan>() }?;
            DeviceRequirements::new(&hal_adapter).ok()?;
            std::panic::catch_unwind(AssertUnwindSafe(|| {
                HardwareBufferImporter::new(device.wgpu_device(), &queue)
            }))
            .ok()
        });
    #[cfg(target_os = "ios")]
    let converter = adapter
        .as_ref()
        .filter(|adapter| adapter.get_info().backend == wgpu::Backend::Metal)
        .map(|_| VideoYcbcrConverter::new(device.wgpu_device()));
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    let _ = adapter;
    *VIDEO_GPU.lock().unwrap() = Some(VideoGpu {
        device,
        queue,
        #[cfg(target_os = "linux")]
        importer,
        #[cfg(target_os = "android")]
        importer,
        #[cfg(target_os = "android")]
        converter: None,
        #[cfg(target_os = "android")]
        import_format: None,
        #[cfg(any(target_os = "linux", target_os = "ios"))]
        converter,
    });
}

pub struct VideoTexture {
    texture: Option<Texture>,
    imported: bool,
    force_opaque: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UploadResult {
    Imported,
    Cpu,
}

#[cfg(target_os = "linux")]
fn linux_device_supports_import(device: &RenderDevice) -> bool {
    let Some(device) = (unsafe { device.wgpu_device().as_hal::<wgpu::hal::api::Vulkan>() }) else {
        return false;
    };
    [
        c"VK_KHR_external_memory_fd",
        c"VK_EXT_external_memory_dma_buf",
        c"VK_EXT_image_drm_format_modifier",
    ]
    .iter()
    .all(|extension| device.enabled_device_extensions().contains(extension))
}

#[cfg(target_os = "linux")]
enum LinuxVideoPlanes {
    Rgb(DmaBufFrame),
    Nv12(DmaBufFrame, DmaBufFrame),
}

#[cfg(target_os = "linux")]
fn linux_video_planes(
    descriptor: &ffmpeg_next::ffi::AVDRMFrameDescriptor,
    width: u32,
    height: u32,
) -> Option<LinuxVideoPlanes> {
    let objects = descriptor
        .objects
        .get(..usize::try_from(descriptor.nb_objects).ok()?)?;
    let layers = descriptor
        .layers
        .get(..usize::try_from(descriptor.nb_layers).ok()?)?;
    if width == 0 || height == 0 || objects.is_empty() {
        return None;
    }
    let plane = |plane: &ffmpeg_next::ffi::AVDRMPlaneDescriptor, width, height, format| {
        let object = objects.get(usize::try_from(plane.object_index).ok()?)?;
        let offset = u32::try_from(plane.offset).ok()?;
        let stride = u32::try_from(plane.pitch).ok().filter(|pitch| *pitch > 0)?;
        if u64::from(offset) >= object.size as u64 {
            return None;
        }
        let fd = unsafe { libc::fcntl(object.fd, libc::F_DUPFD_CLOEXEC, 0) };
        if fd < 0 {
            return None;
        }
        Some(DmaBufFrame::new(
            width,
            height,
            format,
            object.format_modifier,
            vec![DmaBufPlane {
                fd: unsafe { std::os::fd::OwnedFd::from_raw_fd(fd) },
                offset,
                stride,
            }],
            None,
        ))
    };
    match layers {
        [rgb]
            if rgb.nb_planes == 1
                && [*b"AR24", *b"XR24", *b"AB24", *b"XB24"]
                    .map(u32::from_le_bytes)
                    .contains(&rgb.format) =>
        {
            Some(LinuxVideoPlanes::Rgb(plane(
                &rgb.planes[0],
                width,
                height,
                DmaBufFormat::from_fourcc(rgb.format),
            )?))
        }
        [luma, chroma]
            if luma.format == u32::from_le_bytes(*b"R8  ")
                && [*b"GR88", *b"RG88"]
                    .map(u32::from_le_bytes)
                    .contains(&chroma.format)
                && luma.nb_planes == 1
                && chroma.nb_planes == 1 =>
        {
            Some(LinuxVideoPlanes::Nv12(
                plane(&luma.planes[0], width, height, DmaBufFormat::R8)?,
                plane(
                    &chroma.planes[0],
                    width.div_ceil(2),
                    height.div_ceil(2),
                    DmaBufFormat::Rg8,
                )?,
            ))
        }
        [nv12] if nv12.format == u32::from_le_bytes(*b"NV12") && nv12.nb_planes == 2 => {
            Some(LinuxVideoPlanes::Nv12(
                plane(&nv12.planes[0], width, height, DmaBufFormat::R8)?,
                plane(
                    &nv12.planes[1],
                    width.div_ceil(2),
                    height.div_ceil(2),
                    DmaBufFormat::Rg8,
                )?,
            ))
        }
        _ => None,
    }
}

#[cfg(not(target_os = "android"))]
fn import_native(
    native: Option<&ffmpeg_next::frame::Video>,
    width: u32,
    height: u32,
) -> Option<(Texture, bool)> {
    #[cfg(target_os = "linux")]
    {
        let frame =
            native.filter(|frame| frame.format() == ffmpeg_next::format::Pixel::DRM_PRIME)?;
        let descriptor = unsafe {
            (*frame.as_ptr()).data[0]
                .cast::<ffmpeg_next::ffi::AVDRMFrameDescriptor>()
                .as_ref()?
        };
        let planes = linux_video_planes(descriptor, width, height)?;
        let guard = VIDEO_GPU.lock().ok()?;
        let gpu = guard.as_ref()?;
        let importer = gpu.importer.as_ref()?;
        return std::panic::catch_unwind(AssertUnwindSafe(|| {
            let _submission = super::surface_sync::lock();
            match planes {
                LinuxVideoPlanes::Rgb(frame) => {
                    let opaque = frame.format.force_opaque();
                    (importer.copy_to_texture(frame).into(), opaque)
                }
                LinuxVideoPlanes::Nv12(luma, chroma) => {
                    let luma = importer.copy_to_texture(luma);
                    let chroma = importer.copy_to_texture(chroma);
                    let texture = gpu.converter.as_ref().unwrap().convert(
                        gpu.device.wgpu_device(),
                        &gpu.queue,
                        &luma.create_view(&Default::default()),
                        &chroma.create_view(&Default::default()),
                        video_ycbcr_encoding(frame),
                        width,
                        height,
                        [0, 0],
                    );
                    (texture.into(), false)
                }
            }
        }))
        .ok();
    }
    #[cfg(target_os = "ios")]
    {
        let frame =
            native.filter(|frame| frame.format() == ffmpeg_next::format::Pixel::VIDEOTOOLBOX)?;
        let pixel_buffer = unsafe { (*frame.as_ptr()).data[3].cast::<std::ffi::c_void>() };
        if pixel_buffer.is_null() {
            return None;
        }
        let surface = unsafe { CVPixelBufferGetIOSurface(pixel_buffer) };
        if surface.is_null() {
            return None;
        }
        let gpu_guard = VIDEO_GPU.lock().ok()?;
        let gpu = gpu_guard.as_ref()?;
        let converter = gpu.converter.as_ref()?;
        let encoding = video_ycbcr_encoding(frame);
        let _submission = super::surface_sync::lock();
        return std::panic::catch_unwind(AssertUnwindSafe(|| {
            let retained =
                unsafe { Ycbcr420IoSurfaceFrame::retain(std::ptr::NonNull::new(surface).unwrap()) }
                    .with_owner(frame.clone());
            let luma = retained.import(gpu.device.wgpu_device(), Ycbcr420Plane::Luma);
            let chroma = retained.import(gpu.device.wgpu_device(), Ycbcr420Plane::Chroma);
            let luma_view = luma.create_view(&Default::default());
            let chroma_view = chroma.create_view(&Default::default());
            let texture = converter.convert(
                gpu.device.wgpu_device(),
                &gpu.queue,
                &luma_view,
                &chroma_view,
                encoding,
                width,
                height,
                [0, 0],
            );
            (texture, false)
        }))
        .ok();
    }
    #[cfg(not(any(target_os = "linux", target_os = "ios")))]
    {
        let _ = (native, width, height);
        None
    }
}

pub fn upload_video(
    image: &mut Option<VideoTexture>,
    width: u32,
    height: u32,
    pitch: u32,
    native: Option<&ffmpeg_next::frame::Video>,
    #[cfg(target_os = "android")] android: Option<AndroidVideoFrame>,
    pixels: Option<&[u8]>,
) -> Result<UploadResult, String> {
    #[cfg(target_os = "android")]
    let _ = native;
    let image = image.get_or_insert_with(|| VideoTexture {
        texture: None,
        imported: false,
        force_opaque: false,
    });
    if pixels.is_none() {
        #[cfg(not(target_os = "android"))]
        let imported = import_native(native, width, height);
        #[cfg(target_os = "android")]
        let imported = import_android(android, width, height);
        image.texture = imported.as_ref().map(|(texture, _)| texture.clone());
        image.imported = image.texture.is_some();
        image.force_opaque = imported.is_some_and(|(_, force_opaque)| force_opaque);
    }
    if pixels.is_some() && image.imported {
        image.texture = None;
        image.imported = false;
        image.force_opaque = false;
    }
    let guard = VIDEO_GPU.lock().map_err(|e| e.to_string())?;
    let gpu = guard.as_ref().ok_or("video renderer is not initialized")?;
    if image
        .texture
        .as_ref()
        .is_none_or(|texture| texture.width() != width || texture.height() != height)
    {
        image.texture = Some(gpu.device.create_texture(&TextureDescriptor {
            label: Some("Kirikiri video frame"),
            size: Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8Unorm,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        }));
        image.imported = false;
        image.force_opaque = false;
    }
    let Some(pixels) = pixels else {
        if image.imported {
            return Ok(UploadResult::Imported);
        }
        return Err("video frame has no GPU import or CPU pixels".into());
    };
    let row_bytes = width
        .checked_mul(4)
        .ok_or("invalid video frame dimensions")?;
    let required = (pitch as usize)
        .checked_mul(height as usize)
        .ok_or("invalid video frame dimensions")?;
    if width == 0 || height == 0 || pitch < row_bytes || pixels.len() < required {
        return Err("invalid video frame".into());
    }
    let texture = image.texture.as_ref().unwrap();
    if !image.imported {
        gpu.queue.write_texture(
            texture.as_image_copy(),
            pixels,
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(pitch),
                rows_per_image: Some(height),
            },
            texture.size(),
        );
    }
    Ok(UploadResult::Cpu)
}

pub fn color_video(target: u64, image: &VideoTexture, color: [f64; 4]) -> Result<(), String> {
    let texture = image
        .texture
        .as_ref()
        .ok_or("video frame is not uploaded")?;
    super::renderer::color_video(target, texture, color, image.force_opaque)
}

#[cfg(all(test, any(target_os = "linux", target_os = "ios")))]
mod tests {
    use super::*;
    use wgpu_external_frame::{YcbcrEncoding, YcbcrMatrix, YcbcrRange};

    fn block_on<F: std::future::Future>(future: F) -> F::Output {
        struct WakeThread(std::thread::Thread);
        impl std::task::Wake for WakeThread {
            fn wake(self: std::sync::Arc<Self>) {
                self.0.unpark();
            }
            fn wake_by_ref(self: &std::sync::Arc<Self>) {
                self.0.unpark();
            }
        }
        let waker = std::task::Waker::from(std::sync::Arc::new(WakeThread(std::thread::current())));
        let mut context = std::task::Context::from_waker(&waker);
        let mut future = std::pin::pin!(future);
        loop {
            match future.as_mut().poll(&mut context) {
                std::task::Poll::Ready(value) => return value,
                std::task::Poll::Pending => std::thread::park(),
            }
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_drm_planes_validate_layout_and_object_references() {
        use std::os::fd::AsRawFd;
        let file = std::fs::File::open("/dev/null").unwrap();
        let mut descriptor: ffmpeg_next::ffi::AVDRMFrameDescriptor = unsafe { std::mem::zeroed() };
        descriptor.nb_objects = 1;
        descriptor.objects[0].fd = file.as_raw_fd();
        descriptor.objects[0].size = 8192;
        descriptor.nb_layers = 1;
        descriptor.layers[0].format = u32::from_le_bytes(*b"NV12");
        descriptor.layers[0].nb_planes = 2;
        descriptor.layers[0].planes[0].pitch = 64;
        descriptor.layers[0].planes[1].pitch = 64;
        descriptor.layers[0].planes[1].offset = 4096;
        let Some(LinuxVideoPlanes::Nv12(luma, chroma)) = linux_video_planes(&descriptor, 63, 61)
        else {
            panic!("NV12 planes in one object must be supported");
        };
        assert_eq!((luma.width, luma.height), (63, 61));
        assert_eq!((chroma.width, chroma.height), (32, 31));
        assert_eq!(chroma.planes[0].offset, 4096);
        descriptor.layers[0].planes[1].object_index = 1;
        assert!(linux_video_planes(&descriptor, 63, 61).is_none());
        descriptor.layers[0].planes[1].object_index = 0;
        descriptor.layers[0].planes[1].offset = -1;
        assert!(linux_video_planes(&descriptor, 63, 61).is_none());
        descriptor.nb_layers = 5;
        assert!(linux_video_planes(&descriptor, 63, 61).is_none());
    }

    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "requires a VAAPI decoder and Vulkan DMA-BUF import"]
    fn linux_vaapi_frames_import_on_gpu() {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapter = block_on(instance.request_adapter(&Default::default())).unwrap();
        let (device, queue) = block_on(adapter.request_device(&Default::default())).unwrap();
        initialize(
            device.clone().into(),
            RenderQueue::new(queue.clone()),
            Some(RenderAdapter::new(adapter)),
        );
        assert!(
            VIDEO_GPU
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .importer
                .is_some(),
            "DMA-BUF extensions are required"
        );
        let mut decoder = crate::video::Decoder::open(
            include_bytes!("../../../../tests/fixtures/video.mp4").to_vec(),
            true,
        )
        .unwrap();
        assert_eq!(decoder.backend, "vaapi");
        let mut scaler = None;
        let mut image = None;
        let mut count = 0;
        while let Some(frame) = decoder.next(&mut scaler, false).unwrap() {
            assert!(frame.pixels.is_none());
            let native = frame
                .native
                .as_ref()
                .expect("VAAPI must export a DRM frame");
            assert_eq!(native.format(), ffmpeg_next::format::Pixel::DRM_PRIME);
            let descriptor = unsafe {
                &*(*native.as_ptr()).data[0].cast::<ffmpeg_next::ffi::AVDRMFrameDescriptor>()
            };
            assert!(
                linux_video_planes(descriptor, frame.width, frame.height).is_some(),
                "{descriptor:?}"
            );
            assert_eq!(
                upload_video(
                    &mut image,
                    frame.width,
                    frame.height,
                    frame.pitch,
                    Some(native),
                    None,
                )
                .unwrap(),
                UploadResult::Imported
            );
            let texture = image.as_ref().unwrap().texture.as_ref().unwrap();
            let actual = readback_video_texture(&device, &queue, texture);
            let expected_channel = usize::from(frame.time >= 0.5);
            assert!(
                actual[expected_channel] > 240
                    && actual[1 - expected_channel] < 16
                    && actual[2] < 16,
                "pts={} rgba={:?}",
                frame.time,
                &actual[..4]
            );
            count += 1;
        }
        assert_eq!(count, 10);
    }

    #[test]
    fn ycbcr_video_and_full_range_endpoints() {
        for matrix in [YcbcrMatrix::Bt601, YcbcrMatrix::Bt709, YcbcrMatrix::Bt2020] {
            for (range, black, white) in [
                (YcbcrRange::Video, 16.0, 235.0),
                (YcbcrRange::Full, 0.0, 255.0),
            ] {
                let rows = video_ycbcr_rows(YcbcrEncoding { matrix, range });
                for (y, expected) in [(black, 0.0), (white, 1.0)] {
                    let sample = [y / 255.0, 128.0 / 255.0, 128.0 / 255.0, 1.0];
                    for row in rows {
                        let actual: f32 = row.iter().zip(sample).map(|(a, b)| a * b).sum();
                        assert!(
                            (actual - expected).abs() < 0.00001,
                            "{matrix:?} {range:?}: {actual}"
                        );
                    }
                }
            }
        }
    }

    fn readback_video_texture(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        output: &wgpu::Texture,
    ) -> Vec<u8> {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("video conversion test readback"),
            size: u64::from(output.height()) * 256,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            output.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(256),
                    rows_per_image: Some(output.height()),
                },
            },
            output.size(),
        );
        queue.submit([encoder.finish()]);
        let (sender, receiver) = std::sync::mpsc::channel();
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                sender.send(result).unwrap();
            });
        device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(std::time::Duration::from_secs(10)),
            })
            .unwrap();
        receiver
            .recv_timeout(std::time::Duration::from_secs(10))
            .unwrap()
            .unwrap();
        let bytes = buffer.slice(..).get_mapped_range().unwrap().to_vec();
        buffer.unmap();
        bytes
    }

    #[test]
    fn ycbcr_gpu_conversion_handles_color_range_and_partial_workgroups() {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = block_on(instance.request_adapter(&Default::default())).unwrap();
        let (device, queue) = block_on(adapter.request_device(&Default::default())).unwrap();
        let device = &device;
        let queue = &queue;
        let converter = VideoYcbcrConverter::new(device);
        let plane = |width, height, format| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some("video conversion test plane"),
                size: Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format,
                usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
                view_formats: &[],
            })
        };
        let luma = plane(6, 2, TextureFormat::R8Unorm);
        let chroma = plane(3, 1, TextureFormat::Rg8Unorm);
        let luma_view = luma.create_view(&Default::default());
        let chroma_view = chroma.create_view(&Default::default());
        for (matrix, range, black, white, red, u, v) in [
            (YcbcrMatrix::Bt601, YcbcrRange::Video, 16, 235, 81, 90, 240),
            (YcbcrMatrix::Bt709, YcbcrRange::Video, 16, 235, 63, 102, 240),
            (YcbcrMatrix::Bt2020, YcbcrRange::Video, 16, 235, 74, 97, 240),
            (YcbcrMatrix::Bt601, YcbcrRange::Full, 0, 255, 76, 85, 255),
            (YcbcrMatrix::Bt709, YcbcrRange::Full, 0, 255, 54, 99, 255),
            (YcbcrMatrix::Bt2020, YcbcrRange::Full, 0, 255, 67, 92, 255),
        ] {
            queue.write_texture(
                luma.as_image_copy(),
                &[
                    black, black, white, white, red, red, black, black, white, white, red, red,
                ],
                TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(6),
                    rows_per_image: Some(2),
                },
                luma.size(),
            );
            queue.write_texture(
                chroma.as_image_copy(),
                &[128, 128, 128, 128, u, v],
                TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(6),
                    rows_per_image: Some(1),
                },
                chroma.size(),
            );
            let output = converter.convert(
                device,
                queue,
                &luma_view,
                &chroma_view,
                YcbcrEncoding { matrix, range },
                6,
                2,
                [0, 0],
            );
            let bytes = readback_video_texture(device, queue, &output);
            let rows = video_ycbcr_rows(YcbcrEncoding { matrix, range });
            for row in 0..2 {
                for x in 0..6 {
                    let expected = match x {
                        0 | 1 => [0_u8, 0, 0, 255],
                        2 => [255, 255, 255, 255],
                        3 | 4 => {
                            let luma = if x == 3 { white } else { red };
                            let amount = (x as f32 + 0.5) * 0.5 - 1.5;
                            let y = luma as f32 / 255.0;
                            let u = (128.0 + amount * (f32::from(u) - 128.0)) / 255.0;
                            let v = (128.0 + amount * (f32::from(v) - 128.0)) / 255.0;
                            let sample = [y, u, v, 1.0];
                            [
                                (rows[0].iter().zip(sample).map(|(a, b)| a * b).sum::<f32>()
                                    * 255.0) as u8,
                                (rows[1].iter().zip(sample).map(|(a, b)| a * b).sum::<f32>()
                                    * 255.0) as u8,
                                (rows[2].iter().zip(sample).map(|(a, b)| a * b).sum::<f32>()
                                    * 255.0) as u8,
                                255,
                            ]
                        }
                        _ => [255, 0, 0, 255],
                    };
                    let pixel = &bytes[row * 256 + x * 4..row * 256 + x * 4 + 4];
                    for (actual, expected) in pixel.iter().zip(expected) {
                        assert!(
                            (i16::from(*actual) - i16::from(expected)).abs() <= 4,
                            "{matrix:?} {range:?}, {x},{row}: {pixel:?}"
                        );
                    }
                }
            }
        }

        queue.write_texture(
            luma.as_image_copy(),
            &[128; 12],
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(6),
                rows_per_image: Some(2),
            },
            luma.size(),
        );
        queue.write_texture(
            chroma.as_image_copy(),
            &[128, 128, 85, 255, 85, 255],
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(6),
                rows_per_image: Some(1),
            },
            chroma.size(),
        );
        let output = converter.convert(
            device,
            queue,
            &luma_view,
            &chroma_view,
            YcbcrEncoding {
                matrix: YcbcrMatrix::Bt709,
                range: YcbcrRange::Full,
            },
            6,
            2,
            [0, 0],
        );
        let bytes = readback_video_texture(device, queue, &output);
        let reds: Vec<_> = (0..6).map(|x| bytes[x * 4]).collect();
        assert!(reds[0] < reds[1] && reds[1] < reds[2] && reds[2] <= reds[3]);
        assert!(reds[1] - reds[0] < reds[3] - reds[0]);

        let padded_luma = plane(6, 6, TextureFormat::R8Unorm);
        let mut luma_pixels = [0; 36];
        luma_pixels[12..24].fill(128);
        queue.write_texture(
            padded_luma.as_image_copy(),
            &luma_pixels,
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(6),
                rows_per_image: Some(6),
            },
            padded_luma.size(),
        );
        let padded_chroma = plane(3, 3, TextureFormat::Rg8Unorm);
        queue.write_texture(
            padded_chroma.as_image_copy(),
            &[128, 128, 85, 255, 85, 255].repeat(3),
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(6),
                rows_per_image: Some(3),
            },
            padded_chroma.size(),
        );
        let cropped = converter.convert(
            device,
            queue,
            &padded_luma.create_view(&Default::default()),
            &padded_chroma.create_view(&Default::default()),
            YcbcrEncoding {
                matrix: YcbcrMatrix::Bt709,
                range: YcbcrRange::Full,
            },
            4,
            2,
            [2, 2],
        );
        let cropped_pixels = readback_video_texture(device, queue, &cropped);
        for row in 0..2 {
            assert_eq!(
                &cropped_pixels[row * 256..row * 256 + 16],
                &bytes[row * 256 + 8..row * 256 + 24]
            );
        }
    }
}
