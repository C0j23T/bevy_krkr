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

#[cfg(target_os = "linux")]
use wgpu_external_frame::dma_buf::{DmaBufFormat, DmaBufFrame, DmaBufImporter, DmaBufPlane};

static VIDEO_GPU: Mutex<Option<VideoGpu>> = Mutex::new(None);

struct VideoGpu {
    device: RenderDevice,
    queue: RenderQueue,
    #[cfg(target_os = "linux")]
    importer: Option<DmaBufImporter>,
}

pub(crate) fn probe(device: &RenderDevice, adapter: &RenderAdapter) -> &'static str {
    let backend = adapter.get_info().backend;
    #[cfg(target_os = "linux")]
    {
        if backend != wgpu::Backend::Vulkan {
            return "CPU upload: Linux video import requires the Vulkan backend";
        }
        let _ = device;
        return "Vulkan DMA-BUF import available; FFmpeg export format is validated per frame";
    }
    #[cfg(target_os = "ios")]
    {
        let _ = backend;
        return "CPU upload: VideoToolbox-to-Metal texture import is not implemented";
    }
    #[cfg(target_os = "android")]
    {
        let _ = (device, backend);
        return "CPU upload: AHardwareBuffer import requires Vulkan device extensions that Bevy does not enable";
    }
    #[cfg(not(any(target_os = "linux", target_os = "ios", target_os = "android")))]
    {
        let _ = (device, backend);
        "CPU upload: no video frame import backend for this platform"
    }
}

pub(crate) fn initialize(device: RenderDevice, queue: RenderQueue, adapter: Option<RenderAdapter>) {
    #[cfg(target_os = "linux")]
    let importer =
        adapter.map(|adapter| DmaBufImporter::new(device.wgpu_device(), &queue, &adapter));
    #[cfg(not(target_os = "linux"))]
    let _ = adapter;
    *VIDEO_GPU.lock().unwrap() = Some(VideoGpu {
        device,
        queue,
        #[cfg(target_os = "linux")]
        importer,
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

fn import_native(
    native: Option<&ffmpeg_next::frame::Video>,
    width: u32,
    height: u32,
) -> Option<(Texture, bool)> {
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
        if ![
            u32::from_le_bytes(*b"AR24"),
            u32::from_le_bytes(*b"XR24"),
            u32::from_le_bytes(*b"AB24"),
            u32::from_le_bytes(*b"XB24"),
        ]
        .contains(&layer.format)
        {
            return None;
        }
        let format = DmaBufFormat::from_fourcc(layer.format);
        let force_opaque = format.force_opaque();
        let plane = layer.planes[0];
        let object = descriptor.objects.get(plane.object_index as usize)?;
        let fd = unsafe { libc::dup(object.fd) };
        if fd < 0 {
            return None;
        }
        let guard = VIDEO_GPU.lock().ok()?;
        let gpu = guard.as_ref()?;
        let importer = gpu.importer.as_ref()?;
        let frame = DmaBufFrame::new(
            width,
            height,
            format,
            object.format_modifier,
            vec![DmaBufPlane {
                fd: unsafe { std::os::fd::OwnedFd::from_raw_fd(fd) },
                offset: plane.offset as u32,
                stride: plane.pitch as u32,
            }],
            None,
        );
        return std::panic::catch_unwind(AssertUnwindSafe(|| {
            importer.copy_to_texture(frame).into()
        }))
        .ok()
        .map(|texture| (texture, force_opaque));
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
    pixels: Option<&[u8]>,
) -> Result<UploadResult, String> {
    let image = image.get_or_insert_with(|| VideoTexture {
        texture: None,
        imported: false,
        force_opaque: false,
    });
    if pixels.is_none() && (native.is_some() || image.imported) {
        let imported = import_native(native, width, height);
        image.texture = imported.as_ref().map(|(texture, _)| texture.clone());
        image.imported = image.texture.is_some();
        image.force_opaque = imported.is_some_and(|(_, force_opaque)| force_opaque);
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
