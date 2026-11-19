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

pub(super) fn bez_path(elements: &[krkr_shim::layer_ex_draw::PathElement]) -> BezPath {
        let mut path = BezPath::new();
        for element in elements {
            let p = element.points;
            match element.verb {
                0 => path.move_to((p[0], p[1])),
                1 => path.line_to((p[0], p[1])),
                2 => path.curve_to((p[0], p[1]), (p[2], p[3]), (p[4], p[5])),
                3 => path.close_path(),
                _ => unreachable!(),
            }
        }
    path
}

pub(super) struct Backend {
    renderer: vello_gpu::Renderer,
    resources: vello_gpu::Resources,
    pipeline: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
    paint_pipeline: wgpu::ComputePipeline,
    paint_layout: wgpu::BindGroupLayout,
    hatches: HashMap<(u32, [u32; 2]), PooledTexture>,
}

impl Backend {
    fn new(host: &bevy::render::renderer::RenderDevice, queue: &wgpu::Queue, width: u16, height: u16) -> Self {
        let device = host.wgpu_device();
        let (renderer, resources) = vello_gpu::Renderer::new(
            device,
            queue,
            &vello_gpu::RenderTargetConfig {
                format: wgpu::TextureFormat::Rgba8Unorm,
                width,
                height,
            },
        );
        let module = super::super::shader::standalone_shader(host, "krkr/layer_ex_draw_convert.wesl", include_str!("layer_ex_draw_convert.wesl"));
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("LayerExDraw alpha conversion"),
            layout: None,
            module: &module,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        let layout = pipeline.get_bind_group_layout(0);
        let paint_module = super::super::shader::standalone_shader(host, "krkr/layer_ex_draw_paint.wesl", include_str!("layer_ex_draw_paint.wesl"));
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

fn gradient_extend(paint:&Paint) -> Option<Extend> {
    if !paint.blend_positions.is_empty() {return None;}
    if paint.wrap==4 {return Some(Extend::Pad);}
    let [x,y,end_x,end_y]=paint.coordinates;
    let [left,top,width,height]=paint.tile;
    let reflect=if y==end_y && x==left && end_x==left+width {
        matches!(paint.wrap,1|3)
    } else if x==end_x && y==top && end_y==top+height {
        matches!(paint.wrap,2|3)
    } else {return None};
    Some(if reflect {Extend::Reflect} else {Extend::Repeat})
}

fn native_crop(p: [f64; 4], width: u32, height: u32) -> bool {
    p.iter().all(|v| v.fract() == 0.0) && p[0] >= 0.0 && p[1] >= 0.0
        && p[0] + p[2] <= f64::from(width) && p[1] + p[3] <= f64::from(height)
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
        if paint.brush_type == 2 {
            tile = paint.coordinates;
        }
        let ramps: Vec<(f32, [f32; 4])> = if !paint.stops.is_empty() {
            paint.stops.iter().map(|stop| (stop.position as f32, rgba(stop.color))).collect()
        } else if !paint.blend_positions.is_empty() {
            paint.blend_positions.iter().zip(&paint.blend_factors)
                .map(|(&p, &f)| (p as f32, [f as f32; 4])).collect()
        } else {
            vec![(0.0, [0.0; 4]), (1.0, [1.0; 4])]
        };
        let mut data = vec![[0.0; 4]; 9];
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
        data[8] = rgba(paint.colors[1]);
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
                    .or_else(|| paint.surround_colors.last())
                    .copied()
                    .unwrap_or(paint.colors[1]),
            ));
        }
        let data = self.device.create_buffer_with_data(&BufferInitDescriptor {
            label: Some("LayerExDraw procedural paint data"),
            contents: bytemuck::cast_slice(&data),
            usage: BufferUsages::STORAGE,
        });
        let source = self.texture(if paint.brush_type == 2 { paint.image } else { request.target })?;
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
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&source.default_view),
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
        self.draw_layer_ex_clipped(request, &[])
    }

    pub(super) fn draw_layer_ex_clipped(
        &mut self,
        request: &DrawRequest,
        clips: &[(BezPath, Fill)],
    ) -> Result<[f64; 4], String> {
        let target = self.texture(request.target)?;
        let width =
            u16::try_from(target.texture.width()).map_err(|_| "LayerExDraw target too wide")?;
        let height =
            u16::try_from(target.texture.height()).map_err(|_| "LayerExDraw target too tall")?;
        let path = bez_path(&request.path);
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
        for (path, fill) in clips {
            scene.set_fill_rule(*fill);
            scene.push_clip_path(path);
        }
        scene.set_fill_rule(if request.fill_rule == 0 { Fill::EvenOdd } else { Fill::NonZero });
        let transform = Affine::new(request.transform);
        let mut bounds: Option<Rect> = None;
        for (paint_index, paint) in request.paints.iter().enumerate() {
            if paint.brush_type > 4 {
                return Err(format!(
                    "unsupported LayerExDraw brush {}",
                    paint.brush_type
                ));
            }
            let shape_transform = transform * Affine::translate((paint.offset[0], paint.offset[1]));
            let cosmetic = paint.stroke && paint.width == 0.0;
            let transform = if cosmetic {
                Affine::IDENTITY
            } else {
                shape_transform
            };
            let paint_transform = if cosmetic {
                shape_transform * Affine::new(paint.transform)
            } else {
                Affine::new(paint.transform)
            };
            let draw_path = if cosmetic {
                shape_transform * &path
            } else {
                path.clone()
            };
            scene.set_transform(transform);
            scene.set_paint_transform(paint_transform);
            match paint.brush_type {
                0 => scene.set_paint(color(paint.colors[0])),
                1 => {
                    if !hatch::supported(paint.hatch) {
                        return Err("invalid hatch style".into());
                    }
                    scene.set_paint_transform(transform.inverse());
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
                        || paint.surround_colors.len() > paint.brush_points.len()
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
                    let extend=gradient_extend(paint);
                    if extend.is_none() {
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
                        let mut gradient = Gradient::new_linear((p[0], p[1]), (p[2], p[3]))
                            .with_extend(extend.unwrap())
                            .with_interpolation_cs(if paint.brush_flags & 1 != 0 {ColorSpaceTag::LinearSrgb} else {ColorSpaceTag::Srgb})
                            .with_interpolation_alpha_space(vello_gpu::peniko::InterpolationAlphaSpace::Unpremultiplied);
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
                }
                2 => {
                    let source = self.texture(paint.image)?;
                    if source.texture.width() > u16::MAX as u32
                        || source.texture.height() > u16::MAX as u32
                    {
                        return Err("LayerExDraw source texture too large".into());
                    }
                    let p = paint.coordinates;
                    if p[2] <= 0.0 || p[3] <= 0.0 {
                        return Err("texture crop must have positive dimensions".into());
                    }
                    let procedural = !native_crop(p, source.texture.width(), source.texture.height());
                    let region = if procedural {
                        scene.set_paint_transform(transform.inverse());
                        vello_gpu::TextureRegion::Full
                    } else if p == [0.0, 0.0, source.texture.width() as f64, source.texture.height() as f64] {
                        vello_gpu::TextureRegion::Full
                    } else {
                        scene
                            .set_paint_transform(paint_transform * Affine::translate((p[0], p[1])));
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
                        sampler: if procedural { ImageSampler::new().with_quality(vello_gpu::peniko::ImageQuality::Low) } else { ImageSampler::new().with_x_extend(x).with_y_extend(y) },
                    });
                }
                _ => unreachable!(),
            }
            let fill_rule = if request.fill_rule == 0 { Fill::EvenOdd } else { Fill::NonZero };
            let mut include_bounds = |geometry: &BezPath| {
                if !geometry.is_empty() {
                    let rect = (transform * geometry).bounding_box();
                    bounds = Some(bounds.map_or(rect, |bounds| bounds.union(rect)));
                }
            };
            if !paint.stroke {
                scene.fill_path(&draw_path);
                include_bounds(&draw_path);
                continue;
            }
            let mut pen = paint.clone();
            if cosmetic { pen.width = 1.0; }
            if pen.alignment == 1 {
                let mut closed = BezPath::new();
                let mut open = BezPath::new();
                for figure in pen::figures(&draw_path) {
                    if matches!(figure.last(), Some(kurbo::PathEl::ClosePath)) {
                        closed.extend(figure.iter().copied());
                    } else {
                        open.extend(figure.iter().copied());
                    }
                }
                if !closed.is_empty() {
                    scene.push_clip_path(&closed);
                    let outline = pen::outline(&closed, &pen)?;
                    scene.set_fill_rule(Fill::NonZero);
                    scene.fill_path(&outline);
                    scene.pop_clip();
                    include_bounds(&closed);
                }
                if !open.is_empty() {
                    pen.alignment = 0;
                    let outline = pen::outline(&open, &pen)?;
                    scene.set_fill_rule(Fill::NonZero);
                    scene.fill_path(&outline);
                    include_bounds(&outline);
                }
            } else {
                let outline = pen::outline(&draw_path, &pen)?;
                scene.set_fill_rule(Fill::NonZero);
                scene.fill_path(&outline);
                include_bounds(&outline);
            }
            scene.set_fill_rule(fill_rule);
        }
        for _ in clips {
            scene.pop_clip();
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
            .unwrap_or_else(|| Backend::new(&self.device, &self.queue, width, height));
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
                                    pixels.extend(hatch::pixel(paint.hatch, x, y, paint.colors));
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
                        if paint.brush_type == 4 && gradient_extend(paint).is_some() {
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
                        if native_crop(paint.coordinates, source.texture.width(), source.texture.height()) {
                            self.convert_draw_texture(&backend, &source, &source, 3)
                        } else {
                            self.prepare_draw_paint(&backend, request, paint, width.into(), height.into())?
                        }
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
