use super::{KrkrRenderer, PooledTexture};
use krkr_shim::layer_ex_draw::{DrawRequest, Paint};
use vello_gpu::{
    Scene,
    kurbo::{self, Affine, BezPath, Cap, Join, Rect, Shape, Stroke},
    peniko::{Color, Extend, Fill, Gradient, ImageSampler, color::ColorSpaceTag},
};

pub(super) struct Backend {
    renderer: vello_gpu::Renderer,
    resources: vello_gpu::Resources,
    pipeline: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
}

const CONVERT: &str = r#"
@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var previous: texture_2d<f32>;
@group(0) @binding(2) var output: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(3) var<uniform> settings: vec4<u32>;
@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let q = vec2<i32>(gid.xy);
    if (any(gid.xy >= textureDimensions(output))) { return; }
    let s = textureLoad(source, q, 0);
    if (settings.x == 3u) {
        textureStore(output, q, vec4(s.rgb * s.a, s.a));
        return;
    }
    let d = textureLoad(previous, q, 0);
    if (s.a == 0.0) { textureStore(output, q, d); return; }
    let a = s.a + d.a * (1.0 - s.a);
    var result = vec4(0.0);
    if (settings.x == 0u && a > 0.0) {
        result = vec4((s.rgb + d.rgb * d.a * (1.0 - s.a)) / a, a);
    } else if (settings.x == 1u) {
        result = vec4(s.rgb + d.rgb * (1.0 - s.a), a);
    } else if (settings.x == 2u) {
        result = vec4(s.rgb + d.rgb * (1.0 - s.a), 1.0);
    }
    textureStore(output, q, clamp(result, vec4(0.0), vec4(1.0)));
}
"#;

impl Backend {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue, width: u16, height: u16) -> Self {
        let (renderer, resources) = vello_gpu::Renderer::new(
            device,
            queue,
            &vello_gpu::RenderTargetConfig {
                format: wgpu::TextureFormat::Rgba8Unorm,
                width,
                height,
            },
        );
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("LayerExDraw alpha conversion"),
            source: wgpu::ShaderSource::Wgsl(CONVERT.into()),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("LayerExDraw alpha conversion"),
            layout: None,
            module: &module,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        let layout = pipeline.get_bind_group_layout(0);
        Self {
            renderer,
            resources,
            pipeline,
            layout,
        }
    }
}

fn color(argb: u32) -> Color {
    Color::from_rgba8(
        (argb >> 16) as u8,
        (argb >> 8) as u8,
        argb as u8,
        (argb >> 24) as u8,
    )
}

fn cap(value: u32) -> Result<Cap, String> {
    match value {
        0 => Ok(Cap::Butt),
        1 => Ok(Cap::Square),
        2 => Ok(Cap::Round),
        _ => Err(format!("unsupported LayerExDraw cap {value}")),
    }
}

fn stroke(paint: &Paint) -> Result<Stroke, String> {
    if paint.alignment != 0 {
        return Err("inset pen alignment is not supported".into());
    }
    if !paint.dashes.is_empty() && paint.dash_cap != 0 {
        return Err("non-flat dash caps are not supported".into());
    }
    let mut stroke = Stroke::new(paint.width);
    stroke.start_cap = cap(paint.start_cap)?;
    stroke.end_cap = cap(paint.end_cap)?;
    stroke.join = match paint.join {
        0 | 3 => Join::Miter,
        1 => Join::Bevel,
        2 => Join::Round,
        _ => return Err("unsupported LayerExDraw line join".into()),
    };
    stroke.miter_limit = paint.miter_limit;
    stroke.dash_offset = paint.dash_offset * paint.width;
    stroke
        .dash_pattern
        .extend(paint.dashes.iter().map(|value| value * paint.width));
    Ok(stroke)
}

impl KrkrRenderer {
    fn convert_draw_texture(
        &mut self,
        backend: &Backend,
        source: &PooledTexture,
        previous: &PooledTexture,
        mode: u32,
    ) -> PooledTexture {
        use bevy::render::render_resource::{BufferInitDescriptor, BufferUsages};
        let output = self.texture_pool.acquire(
            &self.device,
            source.texture.width(),
            source.texture.height(),
        );
        let settings = self.device.create_buffer_with_data(&BufferInitDescriptor {
            label: Some("LayerExDraw alpha mode"),
            contents: bytemuck::cast_slice(&[mode, 0u32, 0, 0]),
            usage: BufferUsages::UNIFORM,
        });
        let group = self
            .device
            .wgpu_device()
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("LayerExDraw alpha conversion"),
                layout: &backend.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&source.default_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&previous.default_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&output.default_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: settings.as_entire_binding(),
                    },
                ],
            });
        let mut pass = self
            .encoder()
            .begin_compute_pass(&wgpu::ComputePassDescriptor::default());
        pass.set_pipeline(&backend.pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.dispatch_workgroups(
            source.texture.width().div_ceil(8),
            source.texture.height().div_ceil(8),
            1,
        );
        drop(pass);
        output
    }

    pub(super) fn draw_layer_ex(&mut self, request: &DrawRequest) -> Result<[f64; 4], String> {
        let target = self.texture(request.target)?;
        let width =
            u16::try_from(target.texture.width()).map_err(|_| "LayerExDraw target too wide")?;
        let height =
            u16::try_from(target.texture.height()).map_err(|_| "LayerExDraw target too tall")?;
        let mut path = BezPath::new();
        for element in &request.path {
            let p = element.points;
            match element.verb {
                0 => path.move_to((p[0], p[1])),
                1 => path.line_to((p[0], p[1])),
                2 => path.curve_to((p[0], p[1]), (p[2], p[3]), (p[4], p[5])),
                3 => path.close_path(),
                _ => unreachable!(),
            }
        }
        let mut scene = Scene::new(width, height);
        if request.flags & 1 != 0 {
            scene.set_aliasing_threshold(Some(128));
        }
        scene.set_fill_rule(if request.fill_rule == 0 {
            Fill::EvenOdd
        } else {
            Fill::NonZero
        });
        let clip = request.clip;
        let clip_left = clip[0].clamp(0, width.into());
        let clip_top = clip[1].clamp(0, height.into());
        let clip_right = clip[2].clamp(clip_left, width.into());
        let clip_bottom = clip[3].clamp(clip_top, height.into());
        scene.push_clip_rect(&Rect::new(
            clip_left as f64,
            clip_top as f64,
            clip_right as f64,
            clip_bottom as f64,
        ));
        let transform = Affine::new(request.transform);
        let mut bounds: Option<Rect> = None;
        for paint in &request.paints {
            if ![0, 2, 4].contains(&paint.brush_type) {
                return Err(format!(
                    "unsupported LayerExDraw brush {}",
                    paint.brush_type
                ));
            }
            let transform = transform * Affine::translate((paint.offset[0], paint.offset[1]));
            scene.set_transform(transform);
            scene.set_paint_transform(Affine::new(paint.transform));
            match paint.brush_type {
                0 => scene.set_paint(color(paint.colors[0])),
                4 => {
                    let p = paint.coordinates;
                    if p[0] == p[2] && p[1] == p[3] {
                        return Err("zero length gradient".into());
                    }
                    let extend = match paint.wrap {
                        0 => Extend::Repeat,
                        3 => Extend::Reflect,
                        4 => Extend::Pad,
                        _ => return Err(
                            "linear gradient FlipX/FlipY requires a two-dimensional tile contract"
                                .into(),
                        ),
                    };
                    let mut gradient = Gradient::new_linear((p[0], p[1]), (p[2], p[3]))
                        .with_extend(extend)
                        .with_interpolation_cs(ColorSpaceTag::Srgb);
                    if paint.stops.is_empty() {
                        gradient = gradient.with_stops(paint.colors.map(color));
                    } else {
                        let stops: Vec<vello_gpu::peniko::ColorStop> = paint
                            .stops
                            .iter()
                            .map(|s| (s.position as f32, color(s.color)).into())
                            .collect();
                        gradient = gradient.with_stops(stops.as_slice());
                    }
                    scene.set_paint(gradient);
                }
                2 => {
                    let source = self.texture(paint.image)?;
                    if source.texture.width() > u16::MAX as u32
                        || source.texture.height() > u16::MAX as u32
                    {
                        return Err("LayerExDraw source texture too large".into());
                    }
                    let p = paint.coordinates;
                    if p.iter().any(|value| value.fract() != 0.0)
                        || p[0] < 0.0
                        || p[1] < 0.0
                        || p[2] <= 0.0
                        || p[3] <= 0.0
                        || p[0] + p[2] > f64::from(source.texture.width())
                        || p[1] + p[3] > f64::from(source.texture.height())
                    {
                        return Err(
                            "texture crop must be an integer rectangle inside the source".into(),
                        );
                    }
                    let region = if p
                        == [
                            0.0,
                            0.0,
                            source.texture.width() as f64,
                            source.texture.height() as f64,
                        ] {
                        vello_gpu::TextureRegion::Full
                    } else {
                        scene.set_paint_transform(
                            Affine::new(paint.transform) * Affine::translate((p[0], p[1])),
                        );
                        vello_gpu::TextureRegion::Rect(vello_gpu::RectU16::new(
                            p[0] as u16,
                            p[1] as u16,
                            (p[0] + p[2]) as u16,
                            (p[1] + p[3]) as u16,
                        ))
                    };
                    let (x, y) = match paint.wrap {
                        0 => (Extend::Repeat, Extend::Repeat),
                        1 => (Extend::Reflect, Extend::Repeat),
                        2 => (Extend::Repeat, Extend::Reflect),
                        3 => (Extend::Reflect, Extend::Reflect),
                        _ => (Extend::Pad, Extend::Pad),
                    };
                    scene.set_paint(vello_gpu::Image {
                        image: vello_gpu::ImageSource::external_texture(
                            vello_gpu::TextureId(paint.image),
                            region,
                            true,
                        ),
                        sampler: ImageSampler::new().with_x_extend(x).with_y_extend(y),
                    });
                }
                _ => unreachable!(),
            }
            let geometry = if paint.stroke {
                let stroke = stroke(paint)?;
                let outline =
                    kurbo::stroke(path.iter(), &stroke, &kurbo::StrokeOpts::default(), 0.05);
                scene.set_stroke(stroke);
                scene.stroke_path(&path);
                outline
            } else {
                scene.fill_path(&path);
                path.clone()
            };
            let rect = (transform * geometry).bounding_box();
            bounds = Some(bounds.map_or(rect, |bounds| bounds.union(rect)));
        }
        scene.pop_clip();
        if clip_left == clip_right
            || clip_top == clip_bottom
            || request.paints.is_empty()
            || path.is_empty()
        {
            let bounds = bounds.unwrap_or_default();
            return Ok([bounds.x0, bounds.y0, bounds.width(), bounds.height()]);
        }
        self.flush();
        let mut backend = self
            .layer_ex_draw
            .take()
            .unwrap_or_else(|| Backend::new(self.device.wgpu_device(), &self.queue, width, height));
        let result = (|| {
            let mut bindings = vello_gpu::TextureBindings::new();
            let mut sources = Vec::new();
            for paint in request.paints.iter().filter(|paint| paint.brush_type == 2) {
                let source = self.texture(paint.image)?;
                let prepared = self.convert_draw_texture(&backend, &source, &source, 3);
                bindings.insert(
                    vello_gpu::TextureId(paint.image),
                    (*prepared.default_view).clone(),
                );
                sources.push(prepared);
            }
            self.flush();
            let rendered = self
                .texture_pool
                .acquire(&self.device, width.into(), height.into());
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("LayerExDraw Vello render"),
                });
            {
                let _submission = super::super::surface_sync::lock();
                backend
                    .renderer
                    .render(
                        &scene,
                        &mut backend.resources,
                        &mut encoder,
                        vello_gpu::RenderTarget {
                            view: &rendered.default_view,
                            depth_view: None,
                        },
                        &bindings,
                        vello_gpu::RenderOptions::default(),
                    )
                    .map_err(|error| error.to_string())?;
                self.queue.submit([encoder.finish()]);
            }
            let output =
                self.convert_draw_texture(&backend, &rendered, &target, request.alpha_mode);
            if let Some(previous) = self.textures.insert(request.target, output) {
                self.texture_pool.retire(previous);
            }
            self.texture_pool.retire(rendered);
            for source in sources {
                self.texture_pool.retire(source);
            }
            self.flush();
            let bounds = bounds.unwrap_or_default();
            Ok([bounds.x0, bounds.y0, bounds.width(), bounds.height()])
        })();
        self.layer_ex_draw = Some(backend);
        result
    }
}
