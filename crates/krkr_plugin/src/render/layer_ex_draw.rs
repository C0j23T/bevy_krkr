#[path = "layer_ex_draw_hatch.rs"]
mod hatch;
#[path = "layer_ex_draw_pen.rs"]
mod pen;

use super::{KrkrRenderer, PooledTexture};
use krkr_shim::layer_ex_draw::{DrawRequest, Paint};
use std::collections::HashMap;
use vello_gpu::{
    Scene,
    kurbo::{self, Affine, BezPath, Rect, Shape},
    peniko::{Color, Extend, Fill, Gradient, ImageSampler, color::ColorSpaceTag},
};

pub(super) struct Backend {
    renderer: vello_gpu::Renderer,
    resources: vello_gpu::Resources,
    pipeline: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
    paint_pipeline: wgpu::ComputePipeline,
    paint_layout: wgpu::BindGroupLayout,
    hatches: HashMap<(u32, [u32; 2]), PooledTexture>,
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
        let paint_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("LayerExDraw procedural brushes"),
            source: wgpu::ShaderSource::Wgsl(include_str!("layer_ex_draw_paint.wgsl").into()),
        });
        let paint_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("LayerExDraw procedural brushes"),
            layout: None,
            module: &paint_module,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        let paint_layout = paint_pipeline.get_bind_group_layout(0);
        Self {
            renderer,
            resources,
            pipeline,
            layout,
            paint_pipeline,
            paint_layout,
            hatches: HashMap::new(),
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

fn rgba(argb: u32) -> [f32; 4] {
    [
        (argb >> 16) as u8,
        (argb >> 8) as u8,
        argb as u8,
        (argb >> 24) as u8,
    ]
    .map(|v| f32::from(v) / 255.0)
}

fn mix_color(a: u32, b: u32, factor: f64) -> Color {
    let a = rgba(a);
    let b = rgba(b);
    Color::new(std::array::from_fn(|i| {
        a[i] + (b[i] - a[i]) * factor as f32
    }))
}

impl KrkrRenderer {
    fn prepare_draw_paint(
        &mut self,
        backend: &Backend,
        request: &DrawRequest,
        paint: &Paint,
        width: u32,
        height: u32,
    ) -> Result<PooledTexture, String> {
        use bevy::render::render_resource::{BufferInitDescriptor, BufferUsages};
        let transform = Affine::new(request.transform)
            * Affine::translate((paint.offset[0], paint.offset[1]))
            * Affine::new(paint.transform);
        if transform.determinant().abs() < 1e-12 {
            return Err("singular brush transform".into());
        }
        let inv = transform.inverse().as_coeffs();
        let mut tile = paint.tile;
        if paint.brush_type == 3 {
            let (mut left, mut top, mut right, mut bottom) = (
                f64::INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
            );
            for p in &paint.brush_points {
                left = left.min(p.x);
                top = top.min(p.y);
                right = right.max(p.x);
                bottom = bottom.max(p.y);
            }
            tile = [left, top, right - left, bottom - top];
        }
        if paint.wrap < 4 && (tile[2] <= 0.0 || tile[3] <= 0.0) {
            return Err("gradient tile has no area".into());
        }
        let mut ramps: Vec<(f32, [f32; 4])> = Vec::new();
        if !paint.stops.is_empty() {
            ramps.extend(
                paint
                    .stops
                    .iter()
                    .map(|stop| (stop.position as f32, rgba(stop.color))),
            );
        } else if !paint.blend_positions.is_empty() {
            ramps.extend(
                paint
                    .blend_positions
                    .iter()
                    .zip(&paint.blend_factors)
                    .map(|(&p, &f)| {
                        (
                            p as f32,
                            if paint.brush_type == 3 {
                                [f as f32, f as f32, f as f32, 1.0]
                            } else {
                                mix_color(paint.colors[0], paint.colors[1], f).components
                            },
                        )
                    }),
            );
        } else if paint.brush_type == 3 {
            ramps = vec![(0.0, [0.0, 0.0, 0.0, 1.0]), (1.0, [1.0; 4])];
        } else {
            ramps = vec![(0.0, rgba(paint.colors[0])), (1.0, rgba(paint.colors[1]))];
        }
        let mut data = vec![[0.0; 4]; 8];
        data[0] = [
            width as f32,
            paint.brush_type as f32,
            paint.wrap as f32,
            ramps.len() as f32,
        ];
        data[1] = [inv[0] as f32, inv[2] as f32, inv[4] as f32, 0.0];
        data[2] = [inv[1] as f32, inv[3] as f32, inv[5] as f32, 0.0];
        data[3] = tile.map(|v| v as f32);
        data[4] = if paint.brush_type == 3 {
            [paint.center[0] as f32, paint.center[1] as f32, 0.0, 0.0]
        } else {
            paint.coordinates.map(|v| v as f32)
        };
        data[5] = [
            paint.focus[0] as f32,
            paint.focus[1] as f32,
            paint.brush_points.len() as f32,
            0.0,
        ];
        data[6] = rgba(paint.colors[0]);
        data[7] = [
            0.0,
            0.0,
            f32::from(!paint.stops.is_empty()),
            (paint.brush_flags & 1) as f32,
        ];
        for (position, color) in ramps {
            data.push(color);
            data.push([position, 0.0, 0.0, 0.0]);
        }
        for (i, point) in paint.brush_points.iter().enumerate() {
            data.push([point.x as f32, point.y as f32, 0.0, 0.0]);
            data.push(rgba(
                paint
                    .surround_colors
                    .get(i)
                    .or_else(|| paint.surround_colors.first())
                    .copied()
                    .unwrap_or(paint.colors[1]),
            ));
        }
        let data = self.device.create_buffer_with_data(&BufferInitDescriptor {
            label: Some("LayerExDraw procedural paint data"),
            contents: bytemuck::cast_slice(&data),
            usage: BufferUsages::STORAGE,
        });
        let texture = self.texture_pool.acquire(&self.device, width, height);
        let group = self
            .device
            .wgpu_device()
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &backend.paint_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: data.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&texture.default_view),
                    },
                ],
            });
        let mut pass = self
            .encoder()
            .begin_compute_pass(&wgpu::ComputePassDescriptor::default());
        pass.set_pipeline(&backend.paint_pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.dispatch_workgroups(width.div_ceil(8), height.div_ceil(8), 1);
        drop(pass);
        Ok(texture)
    }

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
        for (paint_index, paint) in request.paints.iter().enumerate() {
            if paint.brush_type > 4 {
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
                1 => {
                    if !hatch::supported(paint.hatch) {
                        return Err("invalid hatch style".into());
                    }
                    scene.set_paint(vello_gpu::Image {
                        image: vello_gpu::ImageSource::external_texture(
                            vello_gpu::TextureId(paint_index as u64 + 1),
                            vello_gpu::TextureRegion::Full,
                            true,
                        ),
                        sampler: ImageSampler::new()
                            .with_extend(Extend::Repeat)
                            .with_quality(vello_gpu::peniko::ImageQuality::Low),
                    });
                }
                3 => {
                    if paint.brush_points.len() < 3
                        || paint.brush_points.len() > 4096
                        || (!paint.surround_colors.is_empty()
                            && paint.surround_colors.len() != 1
                            && paint.surround_colors.len() != paint.brush_points.len())
                    {
                        return Err("invalid path gradient boundary or surround colors".into());
                    }
                    scene.set_paint_transform(transform.inverse());
                    scene.set_paint(vello_gpu::Image {
                        image: vello_gpu::ImageSource::external_texture(
                            vello_gpu::TextureId(paint_index as u64 + 1),
                            vello_gpu::TextureRegion::Full,
                            true,
                        ),
                        sampler: ImageSampler::new()
                            .with_quality(vello_gpu::peniko::ImageQuality::Low),
                    });
                }
                4 => {
                    let p = paint.coordinates;
                    if p[0] == p[2] && p[1] == p[3] {
                        return Err("zero length gradient".into());
                    }
                    if paint.wrap < 4 {
                        if paint.tile[2] <= 0.0 || paint.tile[3] <= 0.0 {
                            return Err("linear gradient tile must have positive size".into());
                        }
                        scene.set_paint_transform(transform.inverse());
                        scene.set_paint(vello_gpu::Image {
                            image: vello_gpu::ImageSource::external_texture(
                                vello_gpu::TextureId(paint_index as u64 + 1),
                                vello_gpu::TextureRegion::Full,
                                true,
                            ),
                            sampler: ImageSampler::new()
                                .with_quality(vello_gpu::peniko::ImageQuality::Low),
                        });
                    } else {
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
                            .with_interpolation_cs(if paint.brush_flags & 1 == 0 {
                                ColorSpaceTag::Srgb
                            } else {
                                ColorSpaceTag::LinearSrgb
                            });
                        if paint.stops.is_empty() && !paint.blend_positions.is_empty() {
                            let stops: Vec<vello_gpu::peniko::ColorStop> = paint
                                .blend_positions
                                .iter()
                                .zip(&paint.blend_factors)
                                .map(|(&position, &factor)| {
                                    (
                                        position as f32,
                                        mix_color(paint.colors[0], paint.colors[1], factor),
                                    )
                                        .into()
                                })
                                .collect();
                            gradient = gradient.with_stops(stops.as_slice());
                        } else if paint.stops.is_empty() {
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
                            vello_gpu::TextureId(paint_index as u64 + 1),
                            region,
                            true,
                        ),
                        sampler: ImageSampler::new().with_x_extend(x).with_y_extend(y),
                    });
                }
                _ => unreachable!(),
            }
            let geometry = if paint.stroke {
                if paint.alignment == 1 {
                    if path
                        .iter()
                        .filter(|element| matches!(element, kurbo::PathEl::MoveTo(_)))
                        .count()
                        != path
                            .iter()
                            .filter(|element| matches!(element, kurbo::PathEl::ClosePath))
                            .count()
                    {
                        return Err("inset pen requires closed subpaths".into());
                    }
                    scene.push_clip_path(&path);
                }
                let outline = pen::outline(&path, paint)?;
                scene.set_fill_rule(Fill::NonZero);
                scene.fill_path(&outline);
                scene.set_fill_rule(if request.fill_rule == 0 {
                    Fill::EvenOdd
                } else {
                    Fill::NonZero
                });
                if paint.alignment == 1 {
                    scene.pop_clip();
                    path.clone()
                } else {
                    outline
                }
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
            for (paint_index, paint) in request.paints.iter().enumerate() {
                let prepared = match paint.brush_type {
                    1 => {
                        let key = (paint.hatch, paint.colors);
                        if !backend.hatches.contains_key(&key) {
                            if backend.hatches.len() >= 64 {
                                backend.hatches.clear();
                            }
                            let texture = self.texture_pool.acquire(&self.device, 8, 8);
                            let mut pixels = Vec::with_capacity(256);
                            for y in 0..8 {
                                for x in 0..8 {
                                    let argb = paint.colors
                                        [usize::from(!hatch::foreground(paint.hatch, x, y))];
                                    let alpha = (argb >> 24) as u8;
                                    for channel in
                                        [(argb >> 16) as u8, (argb >> 8) as u8, argb as u8]
                                    {
                                        pixels.push(
                                            ((u16::from(channel) * u16::from(alpha) + 127) / 255)
                                                as u8,
                                        );
                                    }
                                    pixels.push(alpha);
                                }
                            }
                            self.queue.write_texture(
                                texture.texture.as_image_copy(),
                                &pixels,
                                wgpu::TexelCopyBufferLayout {
                                    offset: 0,
                                    bytes_per_row: Some(32),
                                    rows_per_image: None,
                                },
                                texture.texture.size(),
                            );
                            backend.hatches.insert(key, texture);
                        }
                        backend.hatches[&key].clone()
                    }
                    3 | 4 => {
                        if paint.brush_type == 4 && paint.wrap == 4 {
                            continue;
                        }
                        self.prepare_draw_paint(
                            &backend,
                            request,
                            paint,
                            width.into(),
                            height.into(),
                        )?
                    }
                    2 => {
                        let source = self.texture(paint.image)?;
                        self.convert_draw_texture(&backend, &source, &source, 3)
                    }
                    _ => continue,
                };
                bindings.insert(
                    vello_gpu::TextureId(paint_index as u64 + 1),
                    (*prepared.default_view).clone(),
                );
                if paint.brush_type != 1 {
                    sources.push(prepared);
                }
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
