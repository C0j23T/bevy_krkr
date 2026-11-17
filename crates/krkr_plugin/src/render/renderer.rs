// krkr renderer

use std::{
    collections::{HashMap as StdHashMap, VecDeque},
    ffi::CStr,
    hash::{BuildHasher, RandomState},
    sync::{Arc, LazyLock, Mutex, OnceLock},
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
            RawComputePipelineDescriptor, Sampler, ShaderModuleDescriptor, ShaderSource,
            ShaderStages, StorageTextureAccess, TexelCopyBufferInfo, TexelCopyBufferLayout,
            TexelCopyTextureInfo, Texture, TextureAspect, TextureDescriptor, TextureDimension,
            TextureFormat, TextureSampleType, TextureUsages, TextureView, TextureViewDescriptor,
            TextureViewDimension,
        },
        renderer::{RenderAdapter, RenderDevice, RenderQueue},
        texture::GpuImage,
    },
    shader::{Shader, ShaderCache, ShaderCacheSource},
};
use fearless_simd::{Level as SimdLevel, Simd, SimdBase, dispatch as simd_dispatch, u8x16};
use krkr_shim::render::{GpuMethodRequest, RenderCommand};

use super::shader::PluginMethod;
use crate::render::operations::Operation;

static SIMD_LEVEL: LazyLock<SimdLevel> = LazyLock::new(|| SimdLevel::new());

static EMPTY_TEXTURE: OnceLock<PooledTexture> = OnceLock::new();
static NAGANO_RECORDS_DUMMY: OnceLock<Buffer> = OnceLock::new();
static NAGANO_TABLES_DUMMY: OnceLock<Buffer> = OnceLock::new();
static RENDERER_INSTANCE: Mutex<Option<KrkrRenderer>> = Mutex::new(None);

const GAUSSIAN_WEIGHT_CACHE_CAPACITY: usize = 64;
const TEXTURE_POOL_RETIRE_FRAMES: usize = 3;
const TEXTURE_POOL_MAX_PER_SIZE: usize = 2;

static GAUSSIAN_WEIGHTS: LazyLock<Mutex<GaussianWeightCache>> =
    LazyLock::new(|| Mutex::new(GaussianWeightCache::default()));

pub fn initialize(device: RenderDevice, queue: RenderQueue, adapter: Option<RenderAdapter>) {
    #[cfg(feature = "video-ffmpeg")]
    if let Some(adapter) = &adapter {
        bevy::log::info!(
            "video GPU import: {}",
            super::video::probe(&device, adapter)
        );
    }
    #[cfg(feature = "video-ffmpeg")]
    super::video::initialize(device.clone(), queue.clone(), adapter.clone());
    let mut renderer = RENDERER_INSTANCE.lock().unwrap();
    assert!(
        renderer.as_ref().is_none_or(|g| g.textures.is_empty()),
        "a Kirikiri renderer is already active"
    );
    *renderer = Some(KrkrRenderer::new(device, queue));

    unsafe fn dispatch(command: &mut RenderCommand) -> Result<(), String> {
        let mut renderer = RENDERER_INSTANCE.lock().map_err(|e| e.to_string())?;
        let renderer = renderer
            .as_mut()
            .ok_or("Bevy renderer is not initialized")?;
        unsafe { renderer.execute(command) }
    }
    krkr_shim::render::set_render_sink(dispatch);
    krkr_shim::render::set_gpu_method_sink(|request| {
        let mut guard = RENDERER_INSTANCE.lock().map_err(|e| e.to_string())?;
        let renderer = guard.as_mut().ok_or("Bevy renderer is not initialized")?;
        renderer.register_method(request)
    });
}

pub fn flush() {
    if let Some(renderer) = RENDERER_INSTANCE.lock().unwrap().as_mut() {
        renderer.flush();
    }
}

pub fn read_pixels(id: u64) -> Result<Vec<u8>, String> {
    let mut guard = RENDERER_INSTANCE
        .lock()
        .map_err(|error| error.to_string())?;
    let renderer = guard.as_mut().ok_or("Bevy renderer is not initialized")?;
    let texture = renderer.texture(id)?.texture;
    let pitch = texture.width() as usize * 4;
    let mut pixels = vec![0; pitch * texture.height() as usize];
    unsafe { renderer.read(id, 4, pitch, pixels.as_mut_ptr())? };
    Ok(pixels)
}

#[cfg(feature = "video-ffmpeg")]
pub(crate) fn color_video(
    target: u64,
    texture: &Texture,
    color: [f64; 4],
    force_opaque: bool,
) -> Result<(), String> {
    let mut guard = RENDERER_INSTANCE.lock().map_err(|e| e.to_string())?;
    let renderer = guard.as_mut().ok_or("Bevy renderer is not initialized")?;
    let destination = renderer.texture(target)?;
    if destination.texture.size() != texture.size() {
        return Err("video texture dimensions do not match".into());
    }
    let image = PooledTexture {
        texture: texture.clone(),
        default_view: texture.create_view(&TextureViewDescriptor::default()),
        srgb_view: Arc::new(OnceLock::new()),
    };
    let mut command: RenderCommand = unsafe { std::mem::zeroed() };
    command.method = c"VideoColor".as_ptr();
    command.target = target;
    command.geometry = u32::from(force_opaque);
    command.rect = [
        0,
        0,
        destination.texture.width() as i32,
        destination.texture.height() as i32,
    ];
    command.source_rects[0] = command.rect.map(|value| value as f32);
    command.parameters[4..8].copy_from_slice(&color.map(|value| value as f32));
    renderer.draw(&command, Some(image))
}

pub fn image(id: u64) -> Option<GpuImage> {
    let mut renderer = RENDERER_INSTANCE.lock().unwrap();
    let renderer = renderer.as_mut()?;

    let texture = renderer.textures.get(&id)?.clone();
    let texture_descriptor = descriptor(texture.texture.width(), texture.texture.height());
    let view_descriptor = TextureViewDescriptor {
        format: Some(TextureFormat::Rgba8UnormSrgb),
        usage: Some(TextureUsages::TEXTURE_BINDING),
        ..Default::default()
    };
    let image = GpuImage {
        texture_view: texture
            .srgb_view
            .get_or_init(|| texture.texture.create_view(&view_descriptor))
            .clone(),
        texture: texture.texture,
        sampler: renderer.sampler.clone(),
        texture_descriptor,
        texture_view_descriptor: Some(view_descriptor),
        had_data: false,
    };

    renderer.flush();
    renderer.update();

    Some(image)
}

struct KrkrRenderer {
    device: RenderDevice,
    queue: RenderQueue,
    encoder: Option<CommandEncoder>,
    pending: usize,

    textures: HashMap<u64, PooledTexture>,
    texture_pool: TexturePool,
    upload_buffer: Vec<u8>,
    parameter_scratch: Vec<[f32; 4]>,

    pipeline: ComputePipeline,
    plugin_methods: Vec<PluginMethod>,
    used_plugin_opcodes: std::collections::HashSet<u32>,
    layout: BindGroupLayout,
    sampler: Sampler,

    ext_nagano: Option<ExtNaganoExtension>,
}

#[derive(Clone)]
struct PooledTexture {
    texture: Texture,
    default_view: TextureView,
    srgb_view: Arc<OnceLock<TextureView>>,
}

impl PooledTexture {
    fn new(device: &RenderDevice, width: u32, height: u32) -> Self {
        let texture = device.create_texture(&descriptor(width, height));
        Self {
            default_view: texture.create_view(&TextureViewDescriptor::default()),
            texture,
            srgb_view: Arc::new(OnceLock::new()),
        }
    }
}

#[derive(Default)]
struct TexturePool {
    available: StdHashMap<(u32, u32), Vec<PooledTexture>>,
    retired: VecDeque<(usize, PooledTexture)>,
}

impl TexturePool {
    fn acquire(&mut self, device: &RenderDevice, width: u32, height: u32) -> PooledTexture {
        if let Some(texture) = self.available.get_mut(&(width, height)).and_then(Vec::pop) {
            return texture;
        }
        PooledTexture::new(device, width, height)
    }

    fn retire(&mut self, texture: PooledTexture) {
        self.retired
            .push_back((TEXTURE_POOL_RETIRE_FRAMES, texture));
    }

    fn update(&mut self) {
        for _ in 0..self.retired.len() {
            let (frames, texture) = self.retired.pop_front().unwrap();
            if frames <= 1 {
                let size = (texture.texture.width(), texture.texture.height());
                let available = self.available.entry(size).or_default();
                if available.len() < TEXTURE_POOL_MAX_PER_SIZE {
                    available.push(texture);
                }
            } else {
                self.retired.push_back((frames - 1, texture));
            }
        }
    }
}

struct ExtNaganoExtension {
    pipeline: ComputePipeline,
    records: HashMap<u64, Buffer>,
    nagano_tables: Option<(Vec<[f32; 4]>, Buffer)>,
}

fn operation_shaders(
    device: &RenderDevice,
    methods: &[PluginMethod],
) -> (
    ShaderCache<wgpu::ShaderModule, RenderDevice>,
    AssetId<Shader>,
    AssetId<Shader>,
) {
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
    let operations_id = AssetId::<Shader>::Uuid {
        uuid: uuid!("b321f2eb-6264-4520-afe6-1ee741255770"),
    };
    let nagano_id = AssetId::<Shader>::Uuid {
        uuid: uuid!("469f2285-95a9-4659-be73-d7d5bacf54e3"),
    };
    shaders.set_shader(
        operations_id,
        Shader::from_wesl(super::shader::compose(methods), "krkr/operations.wesl"),
    );
    shaders.set_shader(
        nagano_id,
        Shader::from_wesl(include_str!("nagano.wesl"), "krkr/extNagano.wesl"),
    );
    for method in methods {
        shaders.set_shader(
            AssetId::<Shader>::Uuid {
                uuid: bevy::asset::uuid::Uuid::from_u128(u128::from(method.opcode)),
            },
            Shader::from_wesl(method.wesl.clone(), method.path()),
        );
    }
    (shaders, operations_id, nagano_id)
}

impl KrkrRenderer {
    fn new(device: RenderDevice, queue: RenderQueue) -> Self {
        let (mut shaders, operations_id, nagano_id) = operation_shaders(&device, &[]);

        let operations_shader = shaders
            .get(0, operations_id, &[])
            .expect("compile Kirikiri operations shader");
        let ext_nagano_shader = shaders
            .get(0, nagano_id, &[])
            .expect("compile extNagano shader");

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
            let _ = EMPTY_TEXTURE.set(PooledTexture::new(&device, 1, 1));

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

        let sampler = device.create_sampler(&Default::default());
        Self {
            device,
            queue,
            encoder: None,
            pending: 0,
            textures: HashMap::new(),
            texture_pool: TexturePool::default(),
            upload_buffer: Vec::new(),
            parameter_scratch: Vec::new(),
            pipeline,
            plugin_methods: Vec::new(),
            used_plugin_opcodes: Default::default(),
            layout,
            sampler,
            ext_nagano,
        }
    }

    fn register_method(&mut self, request: GpuMethodRequest<'_>) -> Result<u32, String> {
        let mut methods = self.plugin_methods.clone();
        let opcode = match request {
            GpuMethodRequest::Register {
                owner,
                method,
                wesl,
            } => {
                if owner.is_empty() || method.is_empty() || wesl.is_empty() {
                    return Err("GPU method owner, name and WESL must not be empty".into());
                }
                if method == "LayerExImageGaussian"
                    || Operation::to_operation(method).is_ok()
                    || methods.iter().any(|m| m.name == method)
                {
                    return Err(format!(
                        "GPU method {method} is already registered or reserved"
                    ));
                }
                let mut salt = 0u64;
                let random = RandomState::new();
                let opcode = loop {
                    let value = random.hash_one((owner, method, salt)) as u32;
                    if value > 100 && !self.used_plugin_opcodes.contains(&value) {
                        break value;
                    }
                    salt = salt.wrapping_add(1);
                };
                methods.push(PluginMethod {
                    owner: owner.into(),
                    name: method.into(),
                    wesl: wesl.into(),
                    opcode,
                });
                opcode
            }
            GpuMethodRequest::Unregister { owner } => {
                methods.retain(|m| m.owner != owner);
                if methods.len() == self.plugin_methods.len() {
                    return Ok(0);
                }
                0
            }
        };
        let (mut shaders, operations_id, _) = operation_shaders(&self.device, &methods);
        let errors = self
            .device
            .wgpu_device()
            .push_error_scope(wgpu::ErrorFilter::Validation);
        let module = shaders
            .get(0, operations_id, &[])
            .map_err(|e| e.to_string());
        let pipeline = module.map(|module| {
            let layout = self
                .device
                .create_pipeline_layout(&PipelineLayoutDescriptor {
                    label: Some("Kirikiri plugin operations"),
                    bind_group_layouts: &[Some(&self.layout)],
                    immediate_size: 0,
                });
            self.device
                .create_compute_pipeline(&RawComputePipelineDescriptor {
                    label: Some("Kirikiri plugin operations"),
                    layout: Some(&layout),
                    module: &module,
                    entry_point: Some("main"),
                    compilation_options: Default::default(),
                    cache: None,
                })
        });
        let validation = bevy::tasks::block_on(errors.pop());
        if let Some(error) = validation {
            return Err(error.to_string());
        }
        self.pipeline = pipeline?;
        self.plugin_methods = methods;
        if opcode > 100 {
            self.used_plugin_opcodes.insert(opcode);
        }
        Ok(opcode)
    }

    fn texture(&self, id: u64) -> Result<PooledTexture, String> {
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
            let _submission = super::surface_sync::lock();
            self.queue.submit([encoder.finish()]);
        }
        self.pending = 0;
    }

    fn update(&mut self) {
        self.texture_pool.update();
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

        let size = w as usize * h as usize * 4;
        self.upload_buffer.resize(size, 0);
        let bytes = &mut self.upload_buffer;
        bytes.fill(0);
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
            _ => (),
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
            bytes,
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
                let texture = self.texture_pool.acquire(&self.device, w as u32, h as u32);
                if let Some(previous) = self.textures.insert(c.target, texture) {
                    self.texture_pool.retire(previous);
                }
            }
            // drop texture
            1 => {
                if let Some(texture) = self.textures.remove(&c.target) {
                    self.texture_pool.retire(texture);
                }
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
        inputs: [PooledTexture; 4],
        old: PooledTexture,
        data: &[[f32; 4]],
        dispatch_size: [u32; 2],
        output: Option<PooledTexture>,
    ) -> PooledTexture {
        let output = output.unwrap_or_else(|| {
            self.texture_pool
                .acquire(&self.device, old.texture.width(), old.texture.height())
        });

        if !matches!(data[1][0].to_bits(), 40 | 41) {
            self.encoder().copy_texture_to_texture(
                old.texture.as_image_copy(),
                output.texture.as_image_copy(),
                old.texture.size(),
            );
        }

        let views = [
            inputs[0].default_view.clone(),
            inputs[1].default_view.clone(),
            inputs[2].default_view.clone(),
            inputs[3].default_view.clone(),
            output.default_view.clone(),
        ];
        let params = self.device.create_buffer_with_data(&BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(data),
            usage: BufferUsages::STORAGE,
        });
        let entries = [
            BindGroupEntry {
                binding: 0,
                resource: BindingResource::TextureView(&views[0]),
            },
            BindGroupEntry {
                binding: 1,
                resource: BindingResource::TextureView(&views[1]),
            },
            BindGroupEntry {
                binding: 2,
                resource: BindingResource::TextureView(&views[2]),
            },
            BindGroupEntry {
                binding: 3,
                resource: BindingResource::TextureView(&views[3]),
            },
            BindGroupEntry {
                binding: 4,
                resource: BindingResource::TextureView(&views[4]),
            },
            BindGroupEntry {
                binding: 5,
                resource: params.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 6,
                resource: NAGANO_RECORDS_DUMMY.get().unwrap().as_entire_binding(),
            },
            BindGroupEntry {
                binding: 7,
                resource: NAGANO_TABLES_DUMMY.get().unwrap().as_entire_binding(),
            },
        ];
        let group = self.device.create_bind_group(None, &self.layout, &entries);

        let pipeline = self.pipeline.clone();
        {
            let mut pass = self
                .encoder()
                .begin_compute_pass(&ComputePassDescriptor::default());
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups(
                dispatch_size[0].div_ceil(8),
                dispatch_size[1].div_ceil(8),
                1,
            );
        }

        self.pending += 1;

        output
    }

    fn draw(&mut self, c: &RenderCommand, source: Option<PooledTexture>) -> Result<(), String> {
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
            if let Some(plugin) = self.plugin_methods.iter().find(|m| m.name == method) {
                (Operation(plugin.opcode), 0, 0)
            } else {
                Operation::to_operation(method).map_err(|s| s.to_string())?
            };

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
            f32::from_bits(operation.0),
            kind as f32,
            c.parameters[11],
            c.geometry as f32,
        ];
        for i in 0..4 {
            data[2 + i].copy_from_slice(&c.parameters[i * 4..i * 4 + 4]);
        }
        if operation == Operation::LAYER_EX_TEXT_PAINT {
            data[1][2] = 0.0;
        } else if operation != Operation::CHANNEL_SWIZZLE && operation.0 <= 100 {
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

        let mut gaussian_data = std::mem::take(&mut self.parameter_scratch);
        gaussian_data.clear();
        let result: Result<PooledTexture, String> = (|| {
            let nagano_tables_only = operation == Operation::EXT_NAGANO && data[2][0] == 8.0;
            let parameters: &[[f32; 4]] = if matches!(
                operation,
                Operation::LAYER_EX_RASTER | Operation::EXTRANS_WAVE
            ) {
                if c.data.is_null() || c.pitch != old.texture.height() as i32 {
                    return Err("invalid raster shifts".into());
                }
                let shifts =
                    unsafe { std::slice::from_raw_parts(c.data.cast::<f32>(), c.pitch as usize) };
                gaussian_data.extend_from_slice(&data);
                gaussian_data.extend(shifts.iter().map(|s| [*s, 0.0, 0.0, 0.0]));
                &gaussian_data
            } else if operation == Operation::SHRINK_COPY {
                let expected = 2i64
                    * (i64::from(c.rect[2]) - i64::from(c.rect[0]) + i64::from(c.rect[3])
                        - i64::from(c.rect[1]));
                if c.data.is_null() || i64::from(c.pitch) != expected || expected <= 0 {
                    return Err("invalid shrink copy area tables".into());
                }
                gaussian_data.extend_from_slice(&data);
                gaussian_data.extend_from_slice(unsafe {
                    std::slice::from_raw_parts(c.data.cast::<[f32; 4]>(), c.pitch as usize)
                });
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
                gaussian_data.extend_from_slice(&data);
                if c.pitch > 0 {
                    gaussian_data.extend_from_slice(unsafe {
                        std::slice::from_raw_parts(c.data.cast::<[f32; 4]>(), c.pitch as usize)
                    });
                }
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
                let records = unsafe {
                    std::slice::from_raw_parts(c.data.cast::<[f32; 4]>(), expected as usize)
                };
                gaussian_data.extend_from_slice(&data);
                gaussian_data.extend_from_slice(records);
                &gaussian_data
            } else if matches![
                operation,
                Operation::LAYER_EX_IMAGE_GAUSSIAN_H | Operation::LAYER_EX_IMAGE_GAUSSIAN_V
            ] {
                let weights = gaussian_weights(c.parameters[4])?;
                data[3][1] = weights.len() as f32;
                gaussian_data.extend_from_slice(&data);
                gaussian_data.extend(weights.iter().map(|w| [*w, 0.0, 0.0, 0.0]));
                &gaussian_data
            } else {
                &data
            };

            let dispatch_size = [width as u32, height as u32];
            let output = if nagano_tables_only {
                let mut ext_nagano = self.ext_nagano.take().unwrap();
                let tables = unsafe {
                    std::slice::from_raw_parts(c.data.cast::<[f32; 4]>(), c.pitch as usize)
                };
                let output = ext_nagano.universal_3d(
                    self,
                    inputs,
                    old.clone(),
                    parameters,
                    tables,
                    dispatch_size,
                );
                self.ext_nagano = Some(ext_nagano);
                output
            } else if operation == Operation::EXT_NAGANO_BLUR_FADE && c.parameters[8] == 0.0 {
                let mut ext_nagano = self.ext_nagano.take().unwrap();
                let mut blur = |input: PooledTexture, rx: f32, ry: f32| {
                    let mut result = input;
                    let mut scratch = false;
                    if rx > 0.0 {
                        let next = ext_nagano.blur_box_pass(self, result.clone(), rx, true);
                        if scratch {
                            self.texture_pool.retire(result);
                        }
                        result = next;
                        scratch = true;
                    }
                    if ry > 0.0 {
                        let next = ext_nagano.blur_box_pass(self, result.clone(), ry, false);
                        if scratch {
                            self.texture_pool.retire(result);
                        }
                        result = next;
                        scratch = true;
                    }
                    (result, scratch)
                };
                let (source_blurred, source_scratch) = blur(
                    inputs[0].clone(),
                    c.parameters[4] as f32,
                    c.parameters[5] as f32,
                );
                let (destination_blurred, destination_scratch) = blur(
                    inputs[1].clone(),
                    c.parameters[6] as f32,
                    c.parameters[7] as f32,
                );
                self.ext_nagano = Some(ext_nagano);

                let mut final_data = data;
                final_data[3] = [0.0; 4];
                final_data[4][0] = 1.0;
                final_data[4][1] = 1.0;
                let output = self.dispatch_compute(
                    [
                        source_blurred.clone(),
                        destination_blurred.clone(),
                        inputs[2].clone(),
                        old.clone(),
                    ],
                    old.clone(),
                    &final_data,
                    dispatch_size,
                    None,
                );
                if source_scratch {
                    self.texture_pool.retire(source_blurred);
                }
                if destination_scratch {
                    self.texture_pool.retire(destination_blurred);
                }
                output
            } else {
                self.dispatch_compute(inputs, old.clone(), parameters, dispatch_size, None)
            };
            Ok(output)
        })();

        self.parameter_scratch = gaussian_data;

        if let Some(previous) = self.textures.insert(c.target, result?) {
            self.texture_pool.retire(previous);
        }
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
        input: PooledTexture,
        radius: f32,
        horizontal: bool,
    ) -> PooledTexture {
        let mut data = [[0.0; 4]; 26];
        data[0] = [
            0.0,
            0.0,
            input.texture.width() as f32,
            input.texture.height() as f32,
        ];
        data[1][0] = f32::from_bits(if horizontal { 40 } else { 41 });
        data[3][if horizontal { 0 } else { 1 }] = radius;

        let output = renderer.texture_pool.acquire(
            &renderer.device,
            input.texture.width(),
            input.texture.height(),
        );
        let dispatch_size = [input.texture.width(), input.texture.height()];
        renderer.dispatch_compute(
            [input.clone(), input.clone(), input.clone(), input.clone()],
            input,
            &data,
            dispatch_size,
            Some(output.clone()),
        )
    }

    fn universal_3d(
        &mut self,
        renderer: &mut KrkrRenderer,
        inputs: [PooledTexture; 4],
        old: PooledTexture,
        data: &[[f32; 4]],
        tables: &[[f32; 4]],
        dispatch_size: [u32; 2],
    ) -> PooledTexture {
        let (width, height) = (old.texture.width(), old.texture.height());
        let output = renderer
            .texture_pool
            .acquire(&renderer.device, width, height);

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

        let views = [
            inputs[0].default_view.clone(),
            inputs[1].default_view.clone(),
            inputs[2].default_view.clone(),
            inputs[3].default_view.clone(),
            output.default_view.clone(),
        ];
        let params = renderer
            .device
            .create_buffer_with_data(&BufferInitDescriptor {
                label: None,
                contents: bytemuck::cast_slice(data),
                usage: BufferUsages::STORAGE,
            });
        let entries = [
            BindGroupEntry {
                binding: 0,
                resource: BindingResource::TextureView(&views[0]),
            },
            BindGroupEntry {
                binding: 1,
                resource: BindingResource::TextureView(&views[1]),
            },
            BindGroupEntry {
                binding: 2,
                resource: BindingResource::TextureView(&views[2]),
            },
            BindGroupEntry {
                binding: 3,
                resource: BindingResource::TextureView(&views[3]),
            },
            BindGroupEntry {
                binding: 4,
                resource: BindingResource::TextureView(&views[4]),
            },
            BindGroupEntry {
                binding: 5,
                resource: params.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 6,
                resource: records.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 7,
                resource: table_buffer.as_entire_binding(),
            },
        ];
        let group = renderer
            .device
            .create_bind_group(None, &renderer.layout, &entries);

        renderer.encoder().clear_buffer(&records, 0, None);
        let (scatter, pipeline) = (self.pipeline.clone(), renderer.pipeline.clone());
        for (pipeline, groups) in [
            (scatter, (width.div_ceil(8), height.div_ceil(8))),
            (
                pipeline,
                (dispatch_size[0].div_ceil(8), dispatch_size[1].div_ceil(8)),
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

#[derive(Default)]
struct GaussianWeightCache {
    entries: HashMap<u32, Arc<[f32]>>,
    insertion_order: VecDeque<u32>,
}

impl GaussianWeightCache {
    fn get(&self, key: u32) -> Option<Arc<[f32]>> {
        self.entries.get(&key).cloned()
    }

    fn insert(&mut self, key: u32, weights: Arc<[f32]>) {
        if self.entries.contains_key(&key) {
            return;
        }
        if self.entries.len() == GAUSSIAN_WEIGHT_CACHE_CAPACITY {
            if let Some(oldest) = self.insertion_order.pop_front() {
                self.entries.remove(&oldest);
            }
        }
        self.insertion_order.push_back(key);
        self.entries.insert(key, weights);
    }
}

fn gaussian_weights(radius: f32) -> Result<Arc<[f32]>, String> {
    if !radius.is_finite() || radius.abs() > 16384.0 {
        return Err("invalid gaussianBlur radius".into());
    }
    let key = radius.to_bits();
    if let Some(weights) = GAUSSIAN_WEIGHTS.lock().map_err(|e| e.to_string())?.get(key) {
        return Ok(weights);
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
    let weights: Arc<[f32]> = weights.into();
    GAUSSIAN_WEIGHTS
        .lock()
        .map_err(|e| e.to_string())?
        .insert(key, weights.clone());
    Ok(weights)
}
