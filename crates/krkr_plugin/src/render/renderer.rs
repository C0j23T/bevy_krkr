// krkr renderer

use std::{
    ffi::CStr,
    sync::{LazyLock, OnceLock},
};

use bevy::{
    asset::{AssetId, uuid::uuid},
    math::{Mat3, Vec2, Vec3},
    platform::collections::HashMap,
    render::{
        render_resource::{
            BindGroupEntry, BindGroupLayout, BindGroupLayoutEntry, BindingResource, BindingType,
            Buffer, BufferBindingType, BufferDescriptor, BufferInitDescriptor, BufferUsages,
            CommandEncoder, CommandEncoderDescriptor, ComputePassDescriptor, ComputePipeline,
            Extent3d, MapMode, Origin3d, PipelineLayoutDescriptor, PollType,
            RawComputePipelineDescriptor, ShaderModuleDescriptor, ShaderSource, ShaderStages,
            StorageTextureAccess, TexelCopyBufferInfo, TexelCopyBufferLayout, TexelCopyTextureInfo,
            TextureAspect, TextureDescriptor, TextureDimension, TextureFormat, TextureSampleType,
            TextureUsages, TextureViewDescriptor, TextureViewDimension,
        },
        renderer::{RenderDevice, RenderQueue},
        texture::{CachedTexture, TextureCache},
    },
    shader::{Shader, ShaderCache, ShaderCacheSource},
};
use fearless_simd::{Level as SimdLevel, Simd, SimdBase, dispatch as simd_dispatch, u8x16};
use krkr_shim::render::RenderCommand;

use crate::render::operations::Operation;

static SIMD_LEVEL: LazyLock<SimdLevel> = LazyLock::new(|| SimdLevel::new());

static EMPTY_TEXTURE: OnceLock<CachedTexture> = OnceLock::new();
static NAGANO_RECORDS_DUMMY: OnceLock<Buffer> = OnceLock::new();
static NAGANO_TABLES_DUMMY: OnceLock<Buffer> = OnceLock::new();

struct KrkrRenderer {
    device: RenderDevice,
    queue: RenderQueue,
    encoder: Option<CommandEncoder>,
    pending: usize,

    textures: HashMap<u64, CachedTexture>,
    texture_cache: TextureCache,

    pipeline: ComputePipeline,
    layout: BindGroupLayout,

    ext_nagano: Option<ExtNaganoExtension>,
}

struct ExtNaganoExtension {
    pipeline: ComputePipeline,
    records: HashMap<u64, Buffer>,
    nagano_tables: Option<(Vec<[f32; 4]>, Buffer)>,
}

impl KrkrRenderer {
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

        let operations_shader = {
            let shader_id = AssetId::<Shader>::Uuid {
                uuid: uuid!("b321f2eb-6264-4520-afe6-1ee741255770"),
            };
            shaders.set_shader(
                shader_id,
                Shader::from_wesl(super::shader::compose(), "krkr/operations.wesl"),
            );
            shaders
                .get(0, shader_id, &[])
                .expect("compile Kirikiri operations shader")
        };
        let ext_nagano_shader = {
            let shader_id = AssetId::<Shader>::Uuid {
                uuid: uuid!("469f2285-95a9-4659-be73-d7d5bacf54e3"),
            };
            shaders.set_shader(
                shader_id,
                Shader::from_wesl(include_str!("nagano.wesl"), "krkr/extNagano.wesl"),
            );
            shaders
                .get(0, shader_id, &[])
                .expect("compile extNagano shader")
        };

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

        // extNagano
        entries.push(BindGroupLayoutEntry {
            binding: 6,
            visibility: ShaderStages::COMPUTE,
            ty: BindingType::Buffer {
                ty: BufferBindingType::Storage { read_only: false },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        });
        entries.push(BindGroupLayoutEntry {
            binding: 7,
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
            module: &operations_shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        let scatter = device.create_compute_pipeline(&RawComputePipelineDescriptor {
            label: Some("Kirikiri nagano scatter"),
            layout: Some(&pipeline_layout),
            module: &ext_nagano_shader,
            entry_point: Some("nagano_scatter"),
            compilation_options: Default::default(),
            cache: None,
        });

        let ext_nagano = Some(ExtNaganoExtension {
            pipeline: scatter,
            records: HashMap::new(),
            nagano_tables: None,
        });

        {
            let texture = device.create_texture(&descriptor(1, 1));
            let _ = EMPTY_TEXTURE.set(CachedTexture {
                default_view: texture.create_view(&TextureViewDescriptor::default()),
                texture,
            });

            let placeholder = |label| {
                device.create_buffer(&BufferDescriptor {
                    label,
                    size: 16,
                    usage: BufferUsages::STORAGE,
                    mapped_at_creation: false,
                })
            };
            let _ = NAGANO_RECORDS_DUMMY.set(placeholder(Some("Kirikiri placeholder records")));
            let _ = NAGANO_TABLES_DUMMY.set(placeholder(Some("Kirikiri placeholder tables")));
        }

        Self {
            device,
            queue,
            encoder: None,
            pending: 0,
            textures: HashMap::new(),
            texture_cache: TextureCache::default(),
            pipeline,
            layout,
            ext_nagano,
        }
    }

    fn texture(&self, id: u64) -> Result<CachedTexture, String> {
        if id == 0 {
            return EMPTY_TEXTURE
                .get()
                .cloned()
                .ok_or_else(|| String::from("renderer uninitialized"));
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

    fn update(&mut self) {
        self.texture_cache.update();
    }

    unsafe fn upload(&mut self, c: &RenderCommand) -> Result<(), String> {
        let texture = self.texture(c.target)?.texture;
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

        self.flush();
        if c.format == 4 && c.pitch as usize == w as usize * 4 {
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
                unsafe {
                    std::slice::from_raw_parts(c.data.cast::<u8>(), w as usize * h as usize * 4)
                },
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
            return Ok(());
        }

        let mut bytes = Vec::with_capacity(w as usize * h as usize * 4);
        match c.format {
            4 => {
                for row in 0..h as usize {
                    let src = unsafe {
                        std::slice::from_raw_parts(
                            c.data.cast::<u8>().offset(row as isize * c.pitch as isize),
                            w as usize * 4,
                        )
                    };
                    let dst = &mut bytes[row * w as usize * 4..(row + 1) * w as usize * 4];
                    dst.copy_from_slice(src);
                }
            }
            3 => {
                #[inline(always)]
                fn rgb_to_rgba<S: Simd>(simd: S, src: &[u8], dst: &mut [u8]) {
                    let mut src_chunks = src.chunks_exact(48);
                    let mut dst_chunks = dst.chunks_exact_mut(64);

                    for (s, d) in src_chunks.by_ref().zip(dst_chunks.by_ref()) {
                        let a = u8x16::from_slice(simd, &s[0..16]);
                        let b = u8x16::from_slice(simd, &s[16..32]);
                        let c = u8x16::from_slice(simd, &s[32..48]);

                        let m0 = u8x16::from_slice(
                            simd,
                            &[0, 1, 2, 0, 3, 4, 5, 0, 6, 7, 8, 0, 9, 10, 11, 0],
                        );
                        let o0 = a.swizzle_dyn(m0);

                        let m1 = u8x16::from_slice(
                            simd,
                            &[12, 13, 14, 0, 15, 16, 17, 0, 18, 19, 20, 0, 21, 22, 23, 0],
                        );
                        let o1 = a.concat_swizzle_dyn(b, m1);

                        let m2 = u8x16::from_slice(
                            simd,
                            &[8, 9, 10, 0, 11, 12, 13, 0, 14, 15, 16, 0, 17, 18, 19, 0],
                        );
                        let o2 = b.concat_swizzle_dyn(c, m2);

                        let m3 = u8x16::from_slice(
                            simd,
                            &[4, 5, 6, 0, 7, 8, 9, 0, 10, 11, 12, 0, 13, 14, 15, 0],
                        );
                        let o3 = c.swizzle_dyn(m3);

                        let alpha = u8x16::from_slice(
                            simd,
                            &[0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255],
                        );
                        let o0 = o0 | alpha;
                        let o1 = o1 | alpha;
                        let o2 = o2 | alpha;
                        let o3 = o3 | alpha;

                        o0.store_slice(&mut d[0..16]);
                        o1.store_slice(&mut d[16..32]);
                        o2.store_slice(&mut d[32..48]);
                        o3.store_slice(&mut d[48..64]);
                    }

                    for (s, d) in src_chunks
                        .remainder()
                        .chunks_exact(3)
                        .zip(dst_chunks.into_remainder().chunks_exact_mut(4))
                    {
                        d[0] = s[0];
                        d[1] = s[1];
                        d[2] = s[2];
                        d[3] = 255;
                    }
                }

                for row in 0..h as usize {
                    let src = unsafe {
                        std::slice::from_raw_parts(
                            c.data.cast::<u8>().offset(row as isize * c.pitch as isize),
                            w as usize * 3,
                        )
                    };
                    let dst = &mut bytes[row * w as usize * 4..(row + 1) * w as usize * 4];
                    simd_dispatch!(*SIMD_LEVEL, simd => rgb_to_rgba(simd, src, dst));
                }
            }
            1 => {
                #[inline(always)]
                fn gray_to_rgba<S: Simd>(simd: S, src: &[u8], dst: &mut [u8]) {
                    let alpha = u8x16::from_slice(
                        simd,
                        &[0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255],
                    );

                    let mut src_chunks = src.chunks_exact(16);
                    let mut dst_chunks = dst.chunks_exact_mut(64);
                    for (source, target) in src_chunks.by_ref().zip(dst_chunks.by_ref()) {
                        let gray = u8x16::from_slice(simd, source);
                        for (offset, mask) in [
                            [0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3],
                            [4, 4, 4, 4, 5, 5, 5, 5, 6, 6, 6, 6, 7, 7, 7, 7],
                            [8, 8, 8, 8, 9, 9, 9, 9, 10, 10, 10, 10, 11, 11, 11, 11],
                            [
                                12, 12, 12, 12, 13, 13, 13, 13, 14, 14, 14, 14, 15, 15, 15, 15,
                            ],
                        ]
                        .into_iter()
                        .enumerate()
                        {
                            let indices = u8x16::from_slice(simd, &mask);
                            let rgba = gray.swizzle_dyn(indices) | alpha;
                            rgba.store_slice(&mut target[offset * 16..(offset + 1) * 16]);
                        }
                    }

                    for (gray, rgba) in src_chunks
                        .remainder()
                        .iter()
                        .zip(dst_chunks.into_remainder().chunks_exact_mut(4))
                    {
                        rgba.copy_from_slice(&[*gray, *gray, *gray, 255]);
                    }
                }

                for row in 0..h as usize {
                    let src = unsafe {
                        std::slice::from_raw_parts(
                            c.data.cast::<u8>().offset(row as isize * c.pitch as isize),
                            w as usize,
                        )
                    };
                    let dst = &mut bytes[row * w as usize * 4..(row + 1) * w as usize * 4];
                    simd_dispatch!(*SIMD_LEVEL, simd => gray_to_rgba(simd, src, dst));
                }
            }
            _ => {
                bytes.fill(0);
            }
        }

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

    unsafe fn read(
        &mut self,
        id: u64,
        format: u32,
        out_pitch: usize,
        out: *mut u8,
    ) -> Result<(), String> {
        let texture = self.texture(id)?.texture;
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

        let channels = format as usize;
        let width = texture.width() as usize;
        let height = texture.height() as usize;

        match channels {
            4 => {
                if pitch as usize == out_pitch {
                    let size = height * out_pitch;
                    let dst = unsafe { std::slice::from_raw_parts_mut(out, size) };
                    dst.copy_from_slice(&mapped[..size]);
                } else {
                    for (y, bytes) in mapped.chunks_exact(pitch as usize).enumerate() {
                        let dst = unsafe {
                            std::slice::from_raw_parts_mut(out.add(y * out_pitch), width * 4)
                        };
                        dst.copy_from_slice(&bytes[..row as usize]);
                    }
                }
            }
            3 => {
                #[inline(always)]
                fn rgba_to_rgb<S: Simd>(simd: S, src: &[u8], dst: &mut [u8]) {
                    let mut src_chunks = src.chunks_exact(64);
                    let mut dst_chunks = dst.chunks_exact_mut(48);

                    for (s, d) in src_chunks.by_ref().zip(dst_chunks.by_ref()) {
                        let a = u8x16::from_slice(simd, &s[0..16]);
                        let b = u8x16::from_slice(simd, &s[16..32]);
                        let c = u8x16::from_slice(simd, &s[32..48]);
                        let e = u8x16::from_slice(simd, &s[48..64]);

                        a.concat_swizzle_dyn(
                            b,
                            u8x16::from_slice(
                                simd,
                                &[0, 1, 2, 4, 5, 6, 8, 9, 10, 12, 13, 14, 16, 17, 18, 20],
                            ),
                        )
                        .store_slice(&mut d[0..16]);

                        b.concat_swizzle_dyn(
                            c,
                            u8x16::from_slice(
                                simd,
                                &[5, 6, 8, 9, 10, 12, 13, 14, 16, 17, 18, 20, 21, 22, 24, 25],
                            ),
                        )
                        .store_slice(&mut d[16..32]);

                        c.concat_swizzle_dyn(
                            e,
                            u8x16::from_slice(
                                simd,
                                &[
                                    10, 12, 13, 14, 16, 17, 18, 20, 21, 22, 24, 25, 26, 28, 29, 30,
                                ],
                            ),
                        )
                        .store_slice(&mut d[32..48]);
                    }

                    let src = src_chunks.remainder();
                    let dst = dst_chunks.into_remainder();
                    for (rgba, rgb) in src.chunks_exact(4).zip(dst.chunks_exact_mut(3)) {
                        rgb.copy_from_slice(&rgba[..3]);
                    }
                }

                for (y, bytes) in mapped.chunks_exact(pitch as usize).enumerate() {
                    let dst = unsafe {
                        std::slice::from_raw_parts_mut(out.add(y * out_pitch), width * 3)
                    };
                    simd_dispatch!(*SIMD_LEVEL, simd => rgba_to_rgb(simd, &bytes[..row as usize], dst));
                }
            }
            1 => {
                #[inline(always)]
                fn rgba_to_gray<S: Simd>(simd: S, src: &[u8], dst: &mut [u8]) {
                    let mut src_chunks = src.chunks_exact(64);
                    let mut dst_chunks = dst.chunks_exact_mut(48);

                    for (s, ds) in src_chunks.by_ref().zip(dst_chunks.by_ref()) {
                        let a = u8x16::from_slice(simd, &s[0..16]);
                        let b = u8x16::from_slice(simd, &s[16..32]);
                        let c = u8x16::from_slice(simd, &s[32..48]);
                        let d = u8x16::from_slice(simd, &s[48..64]);

                        let ab = a.concat_swizzle_dyn(
                            b,
                            u8x16::from_slice(
                                simd,
                                &[0, 4, 8, 12, 16, 20, 24, 28, 0, 0, 0, 0, 0, 0, 0, 0],
                            ),
                        );

                        let cd = c.concat_swizzle_dyn(
                            d,
                            u8x16::from_slice(
                                simd,
                                &[0, 4, 8, 12, 16, 20, 24, 28, 0, 0, 0, 0, 0, 0, 0, 0],
                            ),
                        );

                        ab.concat_swizzle_dyn(
                            cd,
                            u8x16::from_slice(
                                simd,
                                &[0, 1, 2, 3, 4, 5, 6, 7, 16, 17, 18, 19, 20, 21, 22, 23],
                            ),
                        )
                        .store_slice(ds);
                    }

                    let src = src_chunks.remainder();
                    let dst = dst_chunks.into_remainder();
                    for (gray, rgba) in dst.iter_mut().zip(src.chunks_exact(4)) {
                        *gray = rgba[0];
                    }
                }

                for (y, bytes) in mapped.chunks_exact(pitch as usize).enumerate() {
                    let dst =
                        unsafe { std::slice::from_raw_parts_mut(out.add(y * out_pitch), width) };

                    simd_dispatch!(*SIMD_LEVEL, simd => rgba_to_gray(simd, &bytes[..row as usize], dst));
                }
            }
            _ => unreachable!(),
        }

        drop(mapped);
        buffer.unmap();
        Ok(())
    }

    unsafe fn execute(&mut self, c: &RenderCommand) -> Result<(), String> {
        match c.operation {
            // create texture
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
                    self.texture_cache
                        .get(&self.device, descriptor(w as u32, h as u32)),
                );
            }
            // drop texture
            1 => {
                self.textures.remove(&c.target);
            }
            // upload texture
            2 => unsafe {
                self.upload(c)?;
            },
            // read texture
            3 => {
                let texture = self.texture(c.target)?.texture;
                let channels = c.format as usize;
                if c.data.is_null()
                    || ![1, 3, 4].contains(&channels)
                    || c.pitch < (texture.width() as usize * channels) as i32
                {
                    return Err("invalid texture mapping".into());
                }

                unsafe {
                    self.read(c.target, c.format, c.pitch as usize, c.data as *mut u8)?;
                }
            }
            4 => self.draw(c, None)?,
            5 => self.composite(c)?,
            // reset
            6 => {
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

    #[inline]
    fn composite(&mut self, input: &RenderCommand) -> Result<(), String> {
        const NAMES: [&std::ffi::CStr; 29] = [
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

        let mut c = *input;
        let source = self.texture(c.sources[0])?.texture;
        let layer_type = c.parameters[1] as u32;
        c.method = NAMES
            .get(layer_type as usize)
            .ok_or("invalid layer type")?
            .as_ptr();
        c.sources[1] = c.target;
        c.source_rects[0] = [0.0, 0.0, source.width() as f32, source.height() as f32];
        c.source_rects[1] = c.rect.map(|v| v as f32);

        self.draw(&c, None)
    }

    fn dispatch_compute(
        &mut self,
        inputs: [CachedTexture; 4],
        old: CachedTexture,
        data: &[[f32; 4]],
        rect: [f32; 4],
        output: Option<CachedTexture>,
    ) -> CachedTexture {
        let output = output.unwrap_or_else(|| {
            self.texture_cache.get(
                &self.device,
                descriptor(old.texture.width(), old.texture.height()),
            )
        });

        // not box blur
        if data[1][0] != 40.0 && data[1][0] != 41.0 {
            self.encoder().copy_texture_to_texture(
                old.texture.as_image_copy(),
                output.texture.as_image_copy(),
                old.texture.size(),
            );
        }

        let views: Vec<_> = inputs
            .iter()
            .chain(std::iter::once(&output))
            .map(|texture| texture.default_view.clone())
            .collect();
        let params = self.device.create_buffer_with_data(&BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(data),
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
            resource: params.as_entire_binding(),
        });
        entries.push(BindGroupEntry {
            binding: 6,
            resource: NAGANO_RECORDS_DUMMY.get().unwrap().as_entire_binding(),
        });
        entries.push(BindGroupEntry {
            binding: 7,
            resource: NAGANO_TABLES_DUMMY.get().unwrap().as_entire_binding(),
        });
        let group = self.device.create_bind_group(None, &self.layout, &entries);

        let pipeline = self.pipeline.clone();
        {
            let mut pass = self
                .encoder()
                .begin_compute_pass(&ComputePassDescriptor::default());
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &group, &[]);
            let width = rect[2] - rect[0];
            let height = rect[3] - rect[1];
            pass.dispatch_workgroups((width as u32).div_ceil(8), (height as u32).div_ceil(8), 1);
        }

        self.pending += 1;

        output
    }

    fn draw(&mut self, c: &RenderCommand, source: Option<CachedTexture>) -> Result<(), String> {
        if c.method.is_null() {
            return Err("missing texture operation".into());
        }
        let method = unsafe { CStr::from_ptr(c.method) }
            .to_str()
            .map_err(|e| e.to_string())?;

        if method == "LayerExImageGaussian" {
            let mut pass = *c;
            pass.method = c"LayerExImageGaussianH".as_ptr();
            self.draw(&pass, source.clone())?;
            pass.method = c"LayerExImageGaussianV".as_ptr();
            return self.draw(&pass, source);
        }

        let (operation, kind, blend) =
            Operation::to_operation(method).map_err(|s| s.to_string())?;

        let old = self.texture(c.target)?;
        let inputs = [
            source.map_or_else(|| self.texture(c.sources[0]), Ok)?,
            self.texture(c.sources[1])?,
            self.texture(c.sources[2])?,
            old.clone(),
        ];
        let left = c.rect[0].max(0);
        let top = c.rect[1].max(0);
        let width = c.rect[2].min(old.texture.width() as i32) - left;
        let height = c.rect[3].min(old.texture.height() as i32) - top;
        if width <= 0 || height <= 0 {
            return Ok(());
        }

        let mut data = [[0f32; 4]; 26];
        data[0] = c.rect.map(|v| v as f32);
        data[1] = [
            operation.0 as f32,
            kind as f32,
            c.parameters[11],
            c.geometry as f32,
        ];
        for i in 0..4 {
            data[2 + i].copy_from_slice(&c.parameters[i * 4..i * 4 + 4]);
        }
        if operation == Operation::LAYER_EX_TEXT_PAINT {
            data[1][2] = 0.0;
        } else {
            data[2][1] = blend as f32;
        }
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
                    match mapping(&c.points[0], &c.points[i + 1], c.geometry) {
                        Ok(matrix) => matrix,
                        Err(_) if c.geometry == 4 => return Ok(()),
                        Err(error) => return Err(error),
                    }
                };
                let rows = matrix.transpose().to_cols_array_2d();
                for j in 0..3 {
                    data[17 + i * 3 + j][..3].copy_from_slice(&rows[j]);
                }
            }
        }

        let gaussian_data;
        let nagano_tables_only = operation == Operation::EXT_NAGANO && data[2][0] == 8.0;
        let data: &[[f32; 4]] = if matches!(
            operation,
            Operation::LAYER_EX_RASTER | Operation::EXTRANS_WAVE
        ) {
            if c.data.is_null() || c.pitch != old.texture.height() as i32 {
                return Err("invalid raster shifts".into());
            }
            let shifts =
                unsafe { std::slice::from_raw_parts(c.data.cast::<f32>(), c.pitch as usize) };
            let mut extended = data.to_vec();
            extended.extend(shifts.iter().map(|s| [*s, 0.0, 0.0, 0.0]));
            gaussian_data = extended;
            &gaussian_data
        } else if nagano_tables_only {
            if c.pitch != 512 || c.data.is_null() {
                return Err("invalid Nagano tables".into());
            }
            &data
        } else if operation == Operation::EXT_NAGANO {
            if c.pitch < 0 || c.pitch > 100_000_000 || (c.pitch > 0 && c.data.is_null()) {
                return Err("invalid Nagano records".into());
            }
            let mut extended = data.to_vec();
            if c.pitch > 0 {
                extended.extend_from_slice(unsafe {
                    std::slice::from_raw_parts(c.data.cast::<[f32; 4]>(), c.pitch as usize)
                });
            }
            gaussian_data = extended;
            &gaussian_data
        } else if matches![
            operation,
            Operation::EXTRANS_TURN | Operation::EXTRANS_ROTATE
        ] {
            let expected = if operation == Operation::EXTRANS_TURN {
                64 * 64 * 2
            } else {
                c.parameters[6] as i32 * 10
            };
            if c.data.is_null() || c.pitch != expected || expected <= 0 {
                return Err("invalid transition geometry".into());
            }
            let records =
                unsafe { std::slice::from_raw_parts(c.data.cast::<[f32; 4]>(), expected as usize) };
            let mut extended = data.to_vec();
            extended.extend_from_slice(records);
            gaussian_data = extended;
            &gaussian_data
        } else if matches![
            operation,
            Operation::LAYER_EX_IMAGE_GAUSSIAN_H | Operation::LAYER_EX_IMAGE_GAUSSIAN_V
        ] {
            let weights = gaussian_weights(c.parameters[4])?;
            data[3][1] = weights.len() as f32;
            let mut extended = data.to_vec();
            extended.extend(weights.into_iter().map(|w| [w, 0.0, 0.0, 0.0]));
            gaussian_data = extended;
            &gaussian_data
        } else {
            &data
        };

        let output = if nagano_tables_only {
            let mut ext_nagano = self.ext_nagano.take().unwrap();
            let tables =
                unsafe { std::slice::from_raw_parts(c.data.cast::<[f32; 4]>(), c.pitch as usize) };
            let output = ext_nagano.universal_3d(
                self,
                inputs,
                old.clone(),
                data,
                tables,
                c.rect.map(|v| v as f32),
            );
            self.ext_nagano = Some(ext_nagano);
            output
        } else if operation == Operation::EXT_NAGANO_BLUR_FADE && c.parameters[8] == 0.0 {
            let mut ext_nagano = self.ext_nagano.take().unwrap();
            let mut blur = |input: CachedTexture, rx: f32, ry: f32, first_slot: usize| {
                let mut result = input;
                if rx > 0.0 {
                    result = ext_nagano.blur_box_pass(self, result, rx, true, first_slot);
                }
                if ry > 0.0 {
                    result = ext_nagano.blur_box_pass(self, result, ry, false, first_slot + 1);
                }
                result
            };
            let source_blurred = blur(
                inputs[0].clone(),
                c.parameters[4] as f32,
                c.parameters[5] as f32,
                0,
            );
            let destination_blurred = blur(
                inputs[1].clone(),
                c.parameters[6] as f32,
                c.parameters[7] as f32,
                2,
            );
            self.ext_nagano = Some(ext_nagano);

            let mut final_data = data.to_vec();
            final_data[3] = [0.0; 4];
            final_data[4][0] = 1.0;
            final_data[4][1] = 1.0;
            self.dispatch_compute(
                [
                    source_blurred,
                    destination_blurred,
                    inputs[2].clone(),
                    old.clone(),
                ],
                old.clone(),
                &final_data,
                c.rect.map(|v| v as f32),
                None,
            )
        } else {
            self.dispatch_compute(inputs, old.clone(), data, c.rect.map(|v| v as f32), None)
        };

        self.textures.insert(c.target, output);
        if self.pending >= 64 {
            self.flush();
        }

        Ok(())
    }
}

impl ExtNaganoExtension {
    fn blur_box_pass(
        &mut self,
        renderer: &mut KrkrRenderer,
        input: CachedTexture,
        radius: f32,
        horizontal: bool,
        slot: usize,
    ) -> CachedTexture {
        let mut data = [[0.0; 4]; 26];
        data[0] = [
            0.0,
            0.0,
            input.texture.width() as f32,
            input.texture.height() as f32,
        ];
        data[1][0] = if horizontal { 40.0 } else { 41.0 };
        data[3][if horizontal { 0 } else { 1 }] = radius;

        let output = renderer.texture_cache.get(
            &renderer.device,
            descriptor(input.texture.width(), input.texture.height()),
        );
        renderer.dispatch_compute(
            [input.clone(), input.clone(), input.clone(), input.clone()],
            input,
            &data,
            data[0],
            Some(output.clone()),
        )
    }

    fn universal_3d(
        &mut self,
        renderer: &mut KrkrRenderer,
        inputs: [CachedTexture; 4],
        old: CachedTexture,
        data: &[[f32; 4]],
        tables: &[[f32; 4]],
        rect: [f32; 4],
    ) -> CachedTexture {
        let (width, height) = (old.texture.width(), old.texture.height());
        let output = renderer
            .texture_cache
            .get(&renderer.device, descriptor(width, height));

        renderer.encoder().copy_texture_to_texture(
            old.texture.as_image_copy(),
            output.texture.as_image_copy(),
            old.texture.size(),
        );

        let pixels = width as u64 * height as u64;
        let size = (pixels * 5 + 1) * 4;

        let records = self
            .records
            .entry(size)
            .or_insert_with(|| {
                renderer.device.create_buffer(&BufferDescriptor {
                    label: Some("extNagano records"),
                    size,
                    usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                })
            })
            .clone();
        if self
            .nagano_tables
            .as_ref()
            .is_none_or(|(cached, _)| cached.as_slice() != tables)
        {
            let buffer = renderer
                .device
                .create_buffer_with_data(&BufferInitDescriptor {
                    label: Some("Kirikiri nagano motion tables"),
                    contents: bytemuck::cast_slice(tables),
                    usage: BufferUsages::STORAGE,
                });
            self.nagano_tables = Some((tables.to_vec(), buffer));
        }
        let table_buffer = self
            .nagano_tables
            .as_ref()
            .map(|(_, buffer)| buffer.clone())
            .unwrap();

        let views: Vec<_> = inputs
            .iter()
            .chain(std::iter::once(&output))
            .map(|texture| texture.default_view.clone())
            .collect();
        let params = renderer
            .device
            .create_buffer_with_data(&BufferInitDescriptor {
                label: None,
                contents: bytemuck::cast_slice(data),
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
            resource: params.as_entire_binding(),
        });
        entries.push(BindGroupEntry {
            binding: 6,
            resource: records.as_entire_binding(),
        });
        entries.push(BindGroupEntry {
            binding: 7,
            resource: table_buffer.as_entire_binding(),
        });
        let group = renderer
            .device
            .create_bind_group(None, &renderer.layout, &entries);

        renderer.encoder().clear_buffer(&records, 0, None);
        let (scatter, pipeline) = (self.pipeline.clone(), renderer.pipeline.clone());
        for (pipeline, groups) in [
            (scatter, (width.div_ceil(8), height.div_ceil(8))),
            (
                pipeline,
                (
                    ((rect[2] - rect[0]) as u32).div_ceil(8),
                    ((rect[3] - rect[1]) as u32).div_ceil(8),
                ),
            ),
        ] {
            let mut pass = renderer
                .encoder()
                .begin_compute_pass(&ComputePassDescriptor::default());
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups(groups.0, groups.1, 1);
        }

        renderer.pending += 1;

        output
    }
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

#[inline]
fn mapping(destination: &[f32; 8], source: &[f32; 8], vertices: u32) -> Result<Mat3, String> {
    #[inline]
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

fn gaussian_weights(radius: f32) -> Result<Vec<f32>, String> {
    if !radius.is_finite() || radius.abs() > 16384.0 {
        return Err("invalid gaussianBlur radius".into());
    }
    let sigma = (0.5 * radius).abs() + 0.25;
    let extent = sigma * 2.0;
    let length = (2.0 * (extent - 0.5).ceil() + 1.0) as usize;
    let mid = length / 2;
    let mut weights = vec![0.0; length];
    for i in mid + 1..length {
        let base = i as f32 - mid as f32 - 0.5;
        weights[i] = (1..=50)
            .map(|j| {
                let x = base + 0.02 * j as f32;
                if x <= extent {
                    (-x * x / (2.0 * sigma * sigma)).exp()
                } else {
                    0.0
                }
            })
            .sum::<f32>()
            / 50.0;
        weights[length - 1 - i] = weights[i];
    }
    weights[mid] = (0..=50)
        .map(|j| {
            let x = 0.5 + 0.02 * j as f32;
            (-x * x / (2.0 * sigma * sigma)).exp()
        })
        .sum::<f32>()
        / 51.0;
    let sum: f32 = weights.iter().sum();
    for w in &mut weights {
        *w /= sum;
    }
    Ok(weights)
}
