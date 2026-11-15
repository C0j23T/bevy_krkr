use std::{collections::VecDeque, sync::Mutex};

#[cfg(target_os = "linux")]
use std::os::fd::FromRawFd;

use bevy::render::{
    render_resource::{
        Extent3d, TexelCopyBufferLayout, Texture, TextureDescriptor, TextureDimension,
        TextureFormat, TextureUsages, WgpuFeatures,
    },
    renderer::{RenderAdapter, RenderDevice, RenderQueue},
};

static VIDEO_GPU: Mutex<Option<VideoGpu>> = Mutex::new(None);

struct VideoGpu {
    device: RenderDevice,
    queue: RenderQueue,
}

pub(crate) fn probe(device: &RenderDevice, adapter: &RenderAdapter) -> &'static str {
    let backend = adapter.get_info().backend;
    #[cfg(target_os = "linux")]
    {
        if backend != wgpu::Backend::Vulkan {
            return "CPU upload: Linux video import requires the Vulkan backend";
        }
        let feature = WgpuFeatures::VULKAN_EXTERNAL_MEMORY_DMA_BUF;
        if !adapter.features().contains(feature) {
            return "CPU upload: Vulkan adapter does not support DMA-BUF external memory";
        }
        if !device.features().contains(feature) {
            return "CPU upload: DMA-BUF external memory was not enabled on the Bevy device";
        }
        return "Vulkan DMA-BUF import candidate; decoded-frame export and format need per-frame validation";
    }
    #[cfg(target_os = "ios")]
    {
        let _ = backend;
        return "CPU upload: VideoToolbox-to-Metal texture import is not implemented";
    }
    #[cfg(target_os = "android")]
    {
        let _ = (device, backend);
        return "CPU upload: AHardwareBuffer import is not exposed by the current wgpu-hal integration";
    }
    #[cfg(not(any(target_os = "linux", target_os = "ios", target_os = "android")))]
    {
        let _ = (device, backend);
        "CPU upload: no video frame import backend for this platform"
    }
}

pub(crate) fn initialize(device: RenderDevice, queue: RenderQueue) {
    *VIDEO_GPU.lock().unwrap() = Some(VideoGpu { device, queue });
}

pub struct VideoTexture {
    texture: Option<Texture>,
    imported: bool,
    frames: VecDeque<ffmpeg_next::frame::Video>,
}

fn import_native(
    native: Option<&ffmpeg_next::frame::Video>,
    width: u32,
    height: u32,
) -> Option<Texture> {
    #[cfg(target_os = "linux")]
    {
        let frame =
            native.filter(|frame| frame.format() == ffmpeg_next::format::Pixel::DRM_PRIME)?;
        let descriptor =
            unsafe { &*(frame.data(0).as_ptr() as *const ffmpeg_next::ffi::AVDRMFrameDescriptor) };
        if descriptor.nb_objects < 1
            || descriptor.nb_objects > descriptor.objects.len() as i32
            || descriptor.nb_layers != 1
            || descriptor.layers[0].nb_planes != 1
        {
            return None;
        }
        let layer = descriptor.layers[0];
        // The renderer currently consumes one RGBA texture. Multi-plane NV12 is
        // intentionally left on the CPU path until the YUV shader is added.
        if layer.format != 0x3432_4241 {
            return None;
        }
        let plane = layer.planes[0];
        let object = descriptor.objects.get(plane.object_index as usize)?;
        let fd = unsafe { libc::dup(object.fd) };
        if fd < 0 {
            return None;
        }
        let guard = VIDEO_GPU.lock().ok()?;
        let gpu = guard.as_ref()?;
        let hal = unsafe { gpu.device.wgpu_device().as_hal::<wgpu::hal::api::Vulkan>() }?;
        let view_formats: &[TextureFormat] = &[];
        let descriptor = wgpu_types::TextureDescriptor {
            label: Some("Kirikiri imported video frame"),
            size: Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8Unorm,
            usage: TextureUsages::TEXTURE_BINDING,
            view_formats,
        };
        let hal_descriptor = wgpu::hal::TextureDescriptor {
            label: Some("Kirikiri imported video frame"),
            size: descriptor.size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8Unorm,
            usage: wgpu_types::TextureUses::RESOURCE,
            memory_flags: wgpu::hal::MemoryFlags::empty(),
            view_formats: Vec::new(),
        };
        let imported = unsafe {
            hal.texture_from_dmabuf_fd(
                std::os::fd::OwnedFd::from_raw_fd(fd),
                &hal_descriptor,
                object.format_modifier,
                plane.pitch as u64,
                plane.offset as u64,
            )
            .ok()?
        };
        let imported = unsafe {
            gpu.device
                .wgpu_device()
                .create_texture_from_hal::<wgpu::hal::api::Vulkan>(
                    imported,
                    &descriptor,
                    wgpu_types::TextureUses::RESOURCE,
                )
        };
        return Some(imported.into());
    }
    #[cfg(not(target_os = "linux"))]
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
    pixels: &[u8],
) -> Result<(), String> {
    let row_bytes = width
        .checked_mul(4)
        .ok_or("invalid video frame dimensions")?;
    let required = (pitch as usize)
        .checked_mul(height as usize)
        .ok_or("invalid video frame dimensions")?;
    if width == 0 || height == 0 || pitch < row_bytes || pixels.len() < required {
        return Err("invalid video frame".into());
    }
    let image = image.get_or_insert_with(|| VideoTexture {
        texture: None,
        imported: false,
        frames: VecDeque::new(),
    });
    if native.is_some() || image.imported {
        image.texture = import_native(native, width, height);
        image.imported = image.texture.is_some();
        if image.imported
            && let Some(frame) = native
        {
            image.frames.push_back(frame.clone());
            while image.frames.len() > 4 {
                image.frames.pop_front();
            }
        }
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
    Ok(())
}

pub fn color_video(target: u64, image: &VideoTexture, color: [f64; 4]) -> Result<(), String> {
    let texture = image
        .texture
        .as_ref()
        .ok_or("video frame is not uploaded")?;
    super::renderer::color_video(target, texture, color)
}
