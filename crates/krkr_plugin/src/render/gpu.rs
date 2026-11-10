use std::{collections::HashMap, ffi::CStr, sync::Mutex};

use bevy::math::{Mat3, Vec2, Vec3};
use bevy::render::{
    render_resource::*,
    renderer::{RenderDevice, RenderQueue},
    texture::GpuImage,
};
use bevy::{
    asset::AssetId,
    shader::{Shader, ShaderCache, ShaderCacheSource},
};
use krkr_shim::render::RenderCommand;

static GPU: Mutex<Option<Gpu>> = Mutex::new(None);

pub fn initialize(device: RenderDevice, queue: RenderQueue) {
    let mut gpu = GPU.lock().unwrap();
    assert!(
        gpu.as_ref().is_none_or(|g| g.textures.is_empty()),
        "a Kirikiri renderer is already active"
    );
    *gpu = Some(Gpu::new(device, queue));
    krkr_shim::render::set_render_sink(dispatch);
}

pub fn image(id: u64) -> Option<GpuImage> {
    let mut gpu = GPU.lock().unwrap();
    let gpu = gpu.as_mut()?;
    let texture = gpu.textures.get(&id)?.clone();
    if gpu
        .presentation
        .as_ref()
        .is_none_or(|image| image.texture.size() != texture.size())
    {
        let mut descriptor = descriptor(texture.width(), texture.height());
        descriptor.usage =
            TextureUsages::COPY_DST | TextureUsages::COPY_SRC | TextureUsages::TEXTURE_BINDING;
        let target = gpu.device.create_texture(&descriptor);
        gpu.presentation = Some(GpuImage {
            texture_view: target.create_view(&TextureViewDescriptor {
                format: Some(TextureFormat::Rgba8UnormSrgb),
                ..Default::default()
            }),
            texture: target,
            sampler: gpu.device.create_sampler(&SamplerDescriptor::default()),
            texture_descriptor: descriptor,
            texture_view_descriptor: None,
            had_data: false,
        });
    }
    let presentation = gpu.presentation.as_ref()?.clone();
    gpu.encoder().copy_texture_to_texture(
        texture.as_image_copy(),
        presentation.texture.as_image_copy(),
        texture.size(),
    );
    gpu.flush();
    Some(presentation)
}

pub fn read_pixels(id: u64) -> Result<Vec<u8>, String> {
    GPU.lock()
        .map_err(|e| e.to_string())?
        .as_mut()
        .ok_or("Bevy renderer is not initialized")?
        .read(id)
}

pub fn texture_usage() -> (usize, u64) {
    GPU.lock().unwrap().as_ref().map_or((0, 0), |gpu| {
        (
            gpu.textures.len(),
            gpu.textures
                .values()
                .map(|t| t.width() as u64 * t.height() as u64 * 4)
                .sum(),
        )
    })
}

pub fn flush() {
    if let Some(gpu) = GPU.lock().unwrap().as_mut() {
        gpu.flush();
    }
}

unsafe fn dispatch(command: &mut RenderCommand) -> Result<(), String> {
    let mut gpu = GPU.lock().map_err(|e| e.to_string())?;
    let gpu = gpu.as_mut().ok_or("Bevy renderer is not initialized")?;
    unsafe { gpu.execute(command) }
}

fn descriptor(width: u32, height: u32) -> TextureDescriptor<'static> {
    TextureDescriptor {
        label: Some("Kirikiri texture"),
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
            | TextureUsages::COPY_SRC
            | TextureUsages::COPY_DST,
        view_formats: &[TextureFormat::Rgba8UnormSrgb],
    }
}

struct Gpu {
    device: RenderDevice,
    queue: RenderQueue,
    textures: HashMap<u64, Texture>,
    pipeline: ComputePipeline,
    layout: BindGroupLayout,
    empty: Texture,
    encoder: Option<CommandEncoder>,
    pending: usize,
    presentation: Option<GpuImage>,
}

impl Gpu {
    fn new(device: RenderDevice, queue: RenderQueue) -> Self {
        let mut shaders = ShaderCache::new(device.clone(), |device, source, _| {
            let ShaderCacheSource::Wgsl(source) = source else {
                unreachable!()
            };
            Ok(device
                .wgpu_device()
                .create_shader_module(ShaderModuleDescriptor {
                    label: Some("Kirikiri texture operations"),
                    source: ShaderSource::Wgsl(source.into()),
                }))
        });
        let shader_id = AssetId::<Shader>::default();
        shaders.set_shader(
            shader_id,
            Shader::from_wesl(include_str!("operations.wesl"), "krkr/operations.wesl"),
        );
        let shader = shaders
            .get(0, shader_id, &[])
            .expect("compile Kirikiri WESL");
        let mut entries: Vec<_> = (0..4)
            .map(|binding| BindGroupLayoutEntry {
                binding,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Texture {
                    sample_type: TextureSampleType::Float { filterable: false },
                    view_dimension: TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            })
            .collect();
        entries.push(BindGroupLayoutEntry {
            binding: 4,
            visibility: ShaderStages::COMPUTE,
            ty: BindingType::StorageTexture {
                access: StorageTextureAccess::WriteOnly,
                format: TextureFormat::Rgba8Unorm,
                view_dimension: TextureViewDimension::D2,
            },
            count: None,
        });
        entries.push(BindGroupLayoutEntry {
            binding: 5,
            visibility: ShaderStages::COMPUTE,
            ty: BindingType::Buffer {
                ty: BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        });
        let layout = device.create_bind_group_layout("Kirikiri operations", &entries);
        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_compute_pipeline(&RawComputePipelineDescriptor {
            label: Some("Kirikiri operations"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        let empty = device.create_texture(&descriptor(1, 1));
        Self {
            device,
            queue,
            textures: HashMap::new(),
            pipeline,
            layout,
            empty,
            encoder: None,
            pending: 0,
            presentation: None,
        }
    }
    fn texture(&self, id: u64) -> Result<Texture, String> {
        if id == 0 {
            return Ok(self.empty.clone());
        }
        self.textures
            .get(&id)
            .cloned()
            .ok_or_else(|| format!("unknown texture {id}"))
    }
    fn encoder(&mut self) -> &mut CommandEncoder {
        self.encoder.get_or_insert_with(|| {
            self.device
                .create_command_encoder(&CommandEncoderDescriptor::default())
        })
    }
    fn flush(&mut self) {
        if let Some(encoder) = self.encoder.take() {
            self.queue.submit([encoder.finish()]);
        }
        self.pending = 0;
    }
    unsafe fn execute(&mut self, c: &RenderCommand) -> Result<(), String> {
        match c.operation {
            0 => {
                let [_, _, w, h] = c.rect;
                let limit = self.device.limits().max_texture_dimension_2d;
                if w <= 0
                    || h <= 0
                    || w as u32 > limit
                    || h as u32 > limit
                    || ![1, 3, 4].contains(&c.format)
                {
                    return Err(format!("invalid texture {w}x{h}, format {}", c.format));
                }
                self.textures.insert(
                    c.target,
                    self.device.create_texture(&descriptor(w as u32, h as u32)),
                );
            }
            1 => {
                self.textures.remove(&c.target);
            }
            2 => unsafe {
                self.upload(c)?;
            },
            3 => {
                let bytes = self.read(c.target)?;
                let texture = self.texture(c.target)?;
                let channels = c.format as usize;
                if c.data.is_null()
                    || ![1, 3, 4].contains(&channels)
                    || c.pitch < (texture.width() as usize * channels) as i32
                {
                    return Err("invalid texture mapping".into());
                }
                for y in 0..texture.height() as usize {
                    let row = unsafe {
                        std::slice::from_raw_parts_mut(
                            c.data.cast::<u8>().add(y * c.pitch as usize),
                            texture.width() as usize * channels,
                        )
                    };
                    for (x, pixel) in row.chunks_exact_mut(channels).enumerate() {
                        let i = (y * texture.width() as usize + x) * 4;
                        pixel.copy_from_slice(&bytes[i..i + channels]);
                    }
                }
            }
            4 => self.draw(c)?,
            5 => self.composite(c)?,
            6 => {
                self.presentation = None;
                self.flush();
                self.device
                    .poll(PollType::Wait {
                        submission_index: None,
                        timeout: None,
                    })
                    .map_err(|e| e.to_string())?;
            }
            _ => return Err(format!("invalid texture command {}", c.operation)),
        }
        Ok(())
    }
    unsafe fn upload(&mut self, c: &RenderCommand) -> Result<(), String> {
        let texture = self.texture(c.target)?;
        let [x, y, right, bottom] = c.rect;
        let (w, h) = (right - x, bottom - y);
        if x < 0
            || y < 0
            || w <= 0
            || h <= 0
            || right as u32 > texture.width()
            || bottom as u32 > texture.height()
            || c.data.is_null()
            || ![1, 3, 4].contains(&c.format)
            || c.pitch.unsigned_abs() < w as u32 * c.format
        {
            return Err("invalid texture upload".into());
        }
        let mut bytes = vec![0; w as usize * h as usize * 4];
        for row in 0..h as usize {
            let source = unsafe {
                std::slice::from_raw_parts(
                    c.data.cast::<u8>().offset(row as isize * c.pitch as isize),
                    w as usize * c.format as usize,
                )
            };
            for (col, input) in source.chunks_exact(c.format as usize).enumerate() {
                let out = &mut bytes[(row * w as usize + col) * 4..][..4];
                match input.len() {
                    1 => out.copy_from_slice(&[input[0], input[0], input[0], 255]),
                    3 => out.copy_from_slice(&[input[0], input[1], input[2], 255]),
                    _ => out.copy_from_slice(input),
                }
            }
        }
        self.flush();
        self.queue.write_texture(
            TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: Origin3d {
                    x: x as u32,
                    y: y as u32,
                    z: 0,
                },
                aspect: TextureAspect::All,
            },
            &bytes,
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w as u32 * 4),
                rows_per_image: Some(h as u32),
            },
            Extent3d {
                width: w as u32,
                height: h as u32,
                depth_or_array_layers: 1,
            },
        );
        Ok(())
    }
    fn read(&mut self, id: u64) -> Result<Vec<u8>, String> {
        let texture = self.texture(id)?;
        let row = texture.width() * 4;
        let pitch = row.div_ceil(256) * 256;
        let buffer = self.device.create_buffer(&BufferDescriptor {
            label: Some("Kirikiri pixel read"),
            size: pitch as u64 * texture.height() as u64,
            usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        self.encoder().copy_texture_to_buffer(
            texture.as_image_copy(),
            TexelCopyBufferInfo {
                buffer: &buffer,
                layout: TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(pitch),
                    rows_per_image: Some(texture.height()),
                },
            },
            texture.size(),
        );
        self.flush();
        let (sender, receiver) = std::sync::mpsc::channel();
        buffer.slice(..).map_async(MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        self.device
            .poll(PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .map_err(|e| e.to_string())?;
        receiver
            .recv()
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
        let mapped = buffer
            .slice(..)
            .get_mapped_range()
            .map_err(|e| e.to_string())?;
        let mut result = Vec::with_capacity(row as usize * texture.height() as usize);
        for bytes in mapped.chunks_exact(pitch as usize) {
            result.extend_from_slice(&bytes[..row as usize]);
        }
        drop(mapped);
        buffer.unmap();
        Ok(result)
    }
    fn composite(&mut self, input: &RenderCommand) -> Result<(), String> {
        let mut c = *input;
        let source = self.texture(c.sources[0])?;
        let layer_type = c.parameters[1] as u32;
        let names: [&std::ffi::CStr; 29] = [
            c"AlphaBlend_d",
            c"ConstAlphaBlend_d",
            c"AlphaBlend_d",
            c"AddBlend",
            c"SubBlend",
            c"MulBlend",
            c"AlphaBlend_d",
            c"AlphaBlend_d",
            c"ColorDodgeBlend",
            c"DarkenBlend",
            c"LightenBlend",
            c"ScreenBlend",
            c"AdditiveAlphaBlend_d",
            c"PsAlphaBlend",
            c"PsAddBlend",
            c"PsSubBlend",
            c"PsMulBlend",
            c"PsScreenBlend",
            c"PsOverlayBlend",
            c"PsHardLightBlend",
            c"PsSoftLightBlend",
            c"PsColorDodgeBlend",
            c"PsColorDodge5Blend",
            c"PsColorBurnBlend",
            c"PsLightenBlend",
            c"PsDarkenBlend",
            c"PsDiffBlend",
            c"PsDiff5Blend",
            c"PsExclusionBlend",
        ];
        c.method = names
            .get(layer_type as usize)
            .ok_or("invalid layer type")?
            .as_ptr();
        c.sources[1] = c.target;
        c.source_rects[0] = [0.0, 0.0, source.width() as f32, source.height() as f32];
        c.source_rects[1] = c.rect.map(|v| v as f32);
        self.draw(&c)
    }
    fn draw(&mut self, c: &RenderCommand) -> Result<(), String> {
        self.draw_with_source(c, None)
    }
    fn draw_with_source(&mut self, c: &RenderCommand, source: Option<Texture>) -> Result<(), String> {
        if c.method.is_null() {
            return Err("missing texture operation".into());
        }
        let method = unsafe { CStr::from_ptr(c.method) }
            .to_str()
            .map_err(|e| e.to_string())?;
        let (operation, kind, blend) = operation(method)?;
        let old = self.texture(c.target)?;
        let inputs = [
            source.map_or_else(|| self.texture(c.sources[0]), Ok)?,
            self.texture(c.sources[1])?,
            self.texture(c.sources[2])?,
            old.clone(),
        ];
        let left = c.rect[0].max(0);
        let top = c.rect[1].max(0);
        let width = c.rect[2].min(old.width() as i32) - left;
        let height = c.rect[3].min(old.height() as i32) - top;
        if width <= 0 || height <= 0 {
            return Ok(());
        }
        let mut data = [[0f32; 4]; 26];
        data[0] = c.rect.map(|v| v as f32);
        data[1] = [
            operation as f32,
            kind as f32,
            c.parameters[11],
            c.geometry as f32,
        ];
        for i in 0..4 {
            data[2 + i].copy_from_slice(&c.parameters[i * 4..i * 4 + 4]);
        }
        data[2][1] = blend as f32;
        data[6..9].copy_from_slice(&c.source_rects);
        for i in 0..4 {
            data[9 + i * 2].copy_from_slice(&c.points[i][..4]);
            data[10 + i * 2].copy_from_slice(&c.points[i][4..]);
        }
        if c.geometry != 0 {
            for i in 0..3 {
                let matrix = if c.sources[i] == 0 || (i == 1 && c.sources[i] == c.target) {
                    Mat3::IDENTITY
                } else {
                    mapping(&c.points[0], &c.points[i + 1], c.geometry)?
                };
                let rows = matrix.transpose().to_cols_array_2d();
                for j in 0..3 {
                    data[17 + i * 3 + j][..3].copy_from_slice(&rows[j]);
                }
            }
        }
        let output = self
            .device
            .create_texture(&descriptor(old.width(), old.height()));
        self.encoder().copy_texture_to_texture(
            old.as_image_copy(),
            output.as_image_copy(),
            old.size(),
        );
        let views: Vec<_> = inputs
            .iter()
            .chain(std::iter::once(&output))
            .map(|t| t.create_view(&TextureViewDescriptor::default()))
            .collect();
        let buffer = self.device.create_buffer_with_data(&BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&data),
            usage: BufferUsages::STORAGE,
        });
        let mut entries: Vec<_> = views
            .iter()
            .enumerate()
            .map(|(binding, view)| BindGroupEntry {
                binding: binding as u32,
                resource: BindingResource::TextureView(view),
            })
            .collect();
        entries.push(BindGroupEntry {
            binding: 5,
            resource: buffer.as_entire_binding(),
        });
        let group = self.device.create_bind_group(None, &self.layout, &entries);
        let pipeline = self.pipeline.clone();
        {
            let mut pass = self
                .encoder()
                .begin_compute_pass(&ComputePassDescriptor::default());
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups((width as u32).div_ceil(8), (height as u32).div_ceil(8), 1);
        }
        self.textures.insert(c.target, output);
        self.pending += 1;
        if self.pending >= 64 {
            self.flush();
        }
        Ok(())
    }
}

fn mapping(destination: &[f32; 8], source: &[f32; 8], vertices: u32) -> Result<Mat3, String> {
    fn basis(p: &[f32; 8], count: u32) -> Mat3 {
        let a = Vec2::new(p[0], p[1]);
        let b = Vec2::new(p[2], p[3]);
        let c = Vec2::new(p[4], p[5]);
        if count == 3 {
            return Mat3::from_cols((b - a).extend(0.0), (c - a).extend(0.0), a.extend(1.0));
        }
        let d = Vec2::new(p[6], p[7]);
        let v = a - b - c + d;
        let u = b - d;
        let w = c - d;
        let determinant = u.perp_dot(w);
        let (g, h) = if v.length_squared() < 1e-10 {
            (0.0, 0.0)
        } else {
            (v.perp_dot(w) / determinant, u.perp_dot(v) / determinant)
        };
        Mat3::from_cols(
            Vec3::new(b.x - a.x + g * b.x, b.y - a.y + g * b.y, g),
            Vec3::new(c.x - a.x + h * c.x, c.y - a.y + h * c.y, h),
            a.extend(1.0),
        )
    }
    let a = basis(destination, vertices);
    let b = basis(source, vertices);
    if !a.is_finite() || !b.is_finite() || a.determinant().abs() < 1e-8 {
        return Err("degenerate texture transform".into());
    }
    Ok(b * a.inverse())
}

fn operation(name: &str) -> Result<(u32, u32, u32), String> {
    let kind = if name.ends_with("_d") {
        1
    } else if name.ends_with("_a") {
        2
    } else {
        0
    };
    let base = name
        .trim_end_matches("_HDA")
        .trim_end_matches("_d")
        .trim_end_matches("_a");
    let op = match base {
        "Copy" => 0,
        "VideoColor" => 22,
        "FillARGB" => 1,
        "FillColor" => 2,
        "FillMask" => 3,
        "CopyColor" => 4,
        "CopyMask" => 5,
        "CopyOpaqueImage" => 6,
        "AlphaBlend" | "PerspectiveAlphaBlend" => 7,
        "ConstAlphaBlend" => 8,
        "ConstColorAlphaBlend" => 9,
        "ApplyColorMap" => 10,
        "RemoveConstOpacity" => 11,
        "RemoveOpacity" => 12,
        "ConstAlphaBlend_SD" | "AlphaBlend_SD" => 13,
        "UnivTransBlend" => 14,
        "AlphaToAdditiveAlpha" => 15,
        "AdditiveAlphaToAlpha" => 16,
        "DoGrayScale" => 17,
        "BoxBlur" => 18,
        "BoxBlurAlpha" => return Ok((18, 1, 0)),
        "AdjustGamma" => 19,
        "AdditiveAlphaBlend" => {
            return Ok((
                7,
                if kind == 0 {
                    3
                } else if kind == 1 {
                    5
                } else {
                    4
                },
                0,
            ))
        }
        _ => {
            let (photoshop, base) = base.strip_prefix("Ps").map_or((false, base), |s| (true, s));
            let blend = match base {
                "AlphaBlend" => 0,
                "AddBlend" => 1,
                "SubBlend" => 2,
                "MulBlend" => 3,
                "ScreenBlend" => 4,
                "OverlayBlend" => 5,
                "HardLightBlend" => 6,
                "SoftLightBlend" => 7,
                "ColorDodgeBlend" | "ColorDodge5Blend" => 8,
                "ColorBurnBlend" => 9,
                "LightenBlend" => 10,
                "DarkenBlend" => 11,
                "DiffBlend" | "Diff5Blend" => 12,
                "ExclusionBlend" => 13,
                _ => return Err(format!("unsupported Bevy texture operation {name}")),
            };
            return Ok((if photoshop { 20 } else { 21 }, kind, blend));
        }
    };
    Ok((op, kind, 0))
}

#[cfg(feature = "video-ffmpeg")]
pub(crate) struct VideoTexture(Texture);

#[cfg(feature = "video-ffmpeg")]
pub(crate) fn upload_video(
    image: &mut Option<VideoTexture>,
    width: u32,
    height: u32,
    pitch: u32,
    pixels: &[u8],
) -> Result<(), String> {
    let mut guard = GPU.lock().map_err(|_| "GPU renderer poisoned")?;
    let gpu = guard.as_mut().ok_or("GPU renderer unavailable")?;
    if width == 0
        || height == 0
        || pitch < width.saturating_mul(4)
        || pixels.len() < pitch as usize * height as usize
    {
        return Err("invalid video frame".into());
    }
    if image
        .as_ref()
        .is_none_or(|image| image.0.width() != width || image.0.height() != height)
    {
        *image = Some(VideoTexture(
            gpu.device.create_texture(&descriptor(width, height)),
        ));
    }
    let texture = &image.as_ref().unwrap().0;
    gpu.flush();
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
    Ok(())
}

#[cfg(feature = "video-ffmpeg")]
pub(crate) fn color_video(
    target: u64,
    image: &VideoTexture,
    color: [f64; 4],
) -> Result<(), String> {
    let mut guard = GPU.lock().map_err(|_| "GPU renderer poisoned")?;
    let gpu = guard.as_mut().ok_or("GPU renderer unavailable")?;
    let texture = gpu.texture(target)?;
    if texture.size() != image.0.size() {
        return Err("video texture dimensions do not match".into());
    }
    let mut command: RenderCommand = unsafe { std::mem::zeroed() };
    command.method = c"VideoColor".as_ptr();
    command.target = target;
    command.rect = [0, 0, texture.width() as i32, texture.height() as i32];
    command.source_rects[0] = command.rect.map(|v| v as f32);
    command.parameters[4..8].copy_from_slice(&color.map(|v| v as f32));
    gpu.draw_with_source(&command, Some(image.0.clone()))
}
