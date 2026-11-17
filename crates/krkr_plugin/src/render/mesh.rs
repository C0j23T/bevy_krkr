use bevy::{
    asset::{AssetId, uuid::uuid},
    render::{render_resource::*, renderer::RenderDevice},
    shader::{Shader, ShaderCache, ShaderCacheSource},
};
use krkr_shim::mesh::MeshRequest;

fn raster_shader(device: &RenderDevice) -> std::sync::Arc<wgpu::ShaderModule> {
    let mut shaders = ShaderCache::new(device.clone(), |device, source, _| {
        let ShaderCacheSource::Wgsl(source) = source else {
            unreachable!()
        };
        Ok(device
            .wgpu_device()
            .create_shader_module(ShaderModuleDescriptor {
                label: Some("Kirikiri mesh raster"),
                source: ShaderSource::Wgsl(source.into()),
            }))
    });
    let id = AssetId::<Shader>::Uuid {
        uuid: uuid!("a726401c-982f-4ea6-935b-25840c22b97f"),
    };
    shaders.set_shader(
        id,
        Shader::from_wesl(include_str!("mesh_raster.wesl"), "krkr/mesh_raster.wesl"),
    );
    shaders
        .get(0, id, &[])
        .expect("compile Kirikiri mesh shader")
}

pub(super) struct MeshPipelines {
    pub deform: ComputePipeline,
    pub layout: BindGroupLayout,
    pub prepare: RenderPipeline,
    pub normal: RenderPipeline,
    pub multiply: RenderPipeline,
    pub additive: RenderPipeline,
    pub sampler: Sampler,
}

impl MeshPipelines {
    pub fn new(
        device: &RenderDevice,
        operations: &wgpu::ShaderModule,
        operations_layout: &BindGroupLayout,
    ) -> Self {
        let compute_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("Kirikiri mesh deformation"),
            bind_group_layouts: &[Some(operations_layout)],
            immediate_size: 0,
        });
        let deform = device.create_compute_pipeline(&RawComputePipelineDescriptor {
            label: Some("Kirikiri mesh deformation"),
            layout: Some(&compute_layout),
            module: operations,
            entry_point: Some("deform_mesh"),
            compilation_options: Default::default(),
            cache: None,
        });
        let mut entries: Vec<_> = (0..3)
            .map(|binding| BindGroupLayoutEntry {
                binding,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Texture {
                    sample_type: TextureSampleType::Float { filterable: true },
                    view_dimension: TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            })
            .collect();
        entries.push(BindGroupLayoutEntry {
            binding: 3,
            visibility: ShaderStages::FRAGMENT,
            ty: BindingType::Sampler(SamplerBindingType::Filtering),
            count: None,
        });
        entries.push(BindGroupLayoutEntry {
            binding: 4,
            visibility: ShaderStages::FRAGMENT,
            ty: BindingType::Buffer {
                ty: BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        });
        let layout = device.create_bind_group_layout("Kirikiri mesh raster", &entries);
        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("Kirikiri mesh raster"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let shader = raster_shader(device);
        let pipeline = |label, prepare, blend| {
            device
                .wgpu_device()
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some(label),
                    layout: Some(&pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: Some(if prepare {
                            "prepare_vertex"
                        } else {
                            "mesh_vertex"
                        }),
                        compilation_options: Default::default(),
                        buffers: if prepare {
                            &[]
                        } else {
                            &[Some(wgpu::VertexBufferLayout {
                                array_stride: 16,
                                step_mode: VertexStepMode::Vertex,
                                attributes: &[VertexAttribute {
                                    format: VertexFormat::Float32x4,
                                    offset: 0,
                                    shader_location: 0,
                                }],
                            })]
                        },
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some(if prepare {
                            "prepare_fragment"
                        } else {
                            "mesh_fragment"
                        }),
                        compilation_options: Default::default(),
                        targets: &[Some(ColorTargetState {
                            format: TextureFormat::Rgba8Unorm,
                            blend,
                            write_mask: ColorWrites::ALL,
                        })],
                    }),
                    primitive: PrimitiveState::default(),
                    depth_stencil: None,
                    multisample: MultisampleState::default(),
                    multiview_mask: None,
                    cache: None,
                })
                .into()
        };
        let over = BlendComponent {
            src_factor: BlendFactor::One,
            dst_factor: BlendFactor::OneMinusSrcAlpha,
            operation: BlendOperation::Add,
        };
        Self {
            deform,
            layout,
            prepare: pipeline("Kirikiri mesh prepare", true, None),
            normal: pipeline(
                "Kirikiri mesh alpha",
                false,
                Some(BlendState {
                    color: over,
                    alpha: over,
                }),
            ),
            multiply: pipeline(
                "Kirikiri mesh multiply",
                false,
                Some(BlendState {
                    color: BlendComponent {
                        src_factor: BlendFactor::Dst,
                        dst_factor: BlendFactor::OneMinusSrcAlpha,
                        operation: BlendOperation::Add,
                    },
                    alpha: over,
                }),
            ),
            additive: pipeline(
                "Kirikiri mesh additive",
                false,
                Some(BlendState {
                    color: BlendComponent {
                        src_factor: BlendFactor::One,
                        dst_factor: BlendFactor::One,
                        operation: BlendOperation::Add,
                    },
                    alpha: BlendComponent {
                        src_factor: BlendFactor::One,
                        dst_factor: BlendFactor::One,
                        operation: BlendOperation::Max,
                    },
                }),
            ),
            sampler: device.create_sampler(&SamplerDescriptor {
                mag_filter: FilterMode::Linear,
                min_filter: FilterMode::Linear,
                ..Default::default()
            }),
        }
    }
}

pub(super) fn parameters(request: &MeshRequest, size: [u32; 2]) -> Vec<[f32; 4]> {
    let mut data = vec![
        [0.0, 0.0, size[0] as f32, size[1] as f32],
        [
            f32::from_bits(super::operations::Operation::MESH_RESOLVE.0),
            0.0,
            0.0,
            0.0,
        ],
        [
            request.surfaces.len() as f32,
            request.division_x as f32,
            request.division_y as f32,
            request.blend as f32,
        ],
        request.tint,
        request.color,
        [request.opacity, 0.0, 0.0, 0.0],
    ];
    for surface in &request.surfaces {
        data.extend(
            surface
                .control_points
                .chunks_exact(4)
                .map(|v| [v[0], v[1], v[2], v[3]]),
        );
        for model in &surface.models {
            data.extend(model.chunks_exact(4).map(|v| [v[0], v[1], v[2], v[3]]));
        }
        data.extend([
            surface.placement,
            surface.geometry,
            surface.offset,
            [
                f32::from_bits(surface.kind),
                f32::from_bits(surface.inherit_mask),
                0.0,
                0.0,
            ],
        ]);
    }
    data
}

pub(super) fn indices(x: u32, y: u32) -> Vec<u16> {
    let mut indices = Vec::with_capacity((x * y * 6) as usize);
    for row in 0..y {
        for col in 0..x {
            let a = (row * (x + 1) + col) as u16;
            let c = a + (x + 1) as u16;
            indices.extend([a, a + 1, c, a + 1, c + 1, c]);
        }
    }
    indices
}
