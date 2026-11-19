use bevy::{
    app::PluginsState,
    prelude::*,
    render::renderer::{RenderDevice, RenderQueue},
    window::ExitCondition,
};
use krkr_shim::{
    layer_ex_draw::{BrushPoint, DrawOperation, DrawPaint, PathElement, krkr_layer_ex_draw},
    render::{RenderCommand, krkr_render_dispatch},
};

fn initialize() {
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: None,
                exit_condition: ExitCondition::DontExit,
                ..default()
            })
            .disable::<bevy::winit::WinitPlugin>(),
    );
    while app.plugins_state() != PluginsState::Ready {
        bevy::tasks::tick_global_task_pools_on_main_thread();
        std::thread::yield_now();
    }
    app.finish();
    app.cleanup();
    krkr_plugin::render::initialize(
        app.world().resource::<RenderDevice>().clone(),
        app.world().resource::<RenderQueue>().clone(),
    );
}

fn dispatch(mut command: RenderCommand) {
    assert_eq!(unsafe { krkr_render_dispatch(&mut command) }, 0);
}
fn command(operation: u32, target: u64) -> RenderCommand {
    RenderCommand {
        operation,
        target,
        format: 4,
        rect: [0, 0, 8, 8],
        pitch: 32,
        ..unsafe { std::mem::zeroed() }
    }
}
fn upload(id: u64, color: [u8; 4]) {
    let data = color.repeat(64);
    let mut c = command(2, id);
    c.data = data.as_ptr().cast_mut().cast();
    dispatch(c);
}
fn read(id: u64) -> Vec<u8> {
    let mut data = vec![0; 256];
    let mut c = command(3, id);
    c.data = data.as_mut_ptr().cast();
    dispatch(c);
    data
}
fn near(actual: &[u8], expected: [u8; 4]) {
    assert!(
        actual
            .iter()
            .zip(expected)
            .all(|(&a, b)| a.abs_diff(b) <= 2),
        "{actual:?} != {expected:?}"
    );
}
const IDENTITY: [f64; 6] = [1., 0., 0., 1., 0., 0.];
fn rectangle() -> Vec<PathElement> {
    [
        (0, [0., 0., 0., 0., 0., 0.]),
        (1, [8., 0., 0., 0., 0., 0.]),
        (1, [8., 8., 0., 0., 0., 0.]),
        (1, [0., 8., 0., 0., 0., 0.]),
        (3, [0.; 6]),
    ]
    .into_iter()
    .map(|(verb, points)| PathElement {
        verb,
        reserved: 0,
        points,
    })
    .collect()
}
fn paint(color: u32) -> DrawPaint {
    DrawPaint {
        colors: [color; 2],
        width: 1.,
        miter_limit: 10.,
        paint_transform: IDENTITY,
        ..unsafe { std::mem::zeroed() }
    }
}
fn draw(
    path: &[PathElement],
    paints: &[DrawPaint],
    transform: [f64; 6],
    clip: [i32; 4],
    alpha_mode: u32,
) -> [f64; 4] {
    let mut operation = DrawOperation {
        target: 1,
        clip,
        transform,
        path: path.as_ptr(),
        path_count: path.len() as u32,
        fill_rule: 0,
        paints: paints.as_ptr(),
        paint_count: paints.len() as u32,
        alpha_mode,
        flags: 0,
        reserved: 0,
        bounds: [0.; 4],
    };
    assert_eq!(unsafe { krkr_layer_ex_draw(&mut operation) }, 0);
    operation.bounds
}

#[test]
fn vello_paths_alpha_clip_transform_gradient_stroke_and_external_gpu_image() {
    initialize();
    for id in [1, 2] {
        dispatch(command(0, id));
    }
    let path = rectangle();
    upload(1, [0; 4]);
    assert_eq!(
        draw(&path, &[paint(0x80ff0000)], IDENTITY, [0, 0, 8, 8], 0),
        [0., 0., 8., 8.]
    );
    for p in read(1).chunks_exact(4) {
        near(p, [255, 0, 0, 128]);
    }
    draw(&path, &[paint(0x800000ff)], IDENTITY, [0, 0, 8, 8], 0);
    for p in read(1).chunks_exact(4) {
        near(p, [85, 0, 170, 192]);
    }
    upload(1, [0; 4]);
    draw(&path, &[paint(0x80ff0000)], IDENTITY, [0, 0, 8, 8], 1);
    for p in read(1).chunks_exact(4) {
        near(p, [128, 0, 0, 128]);
    }
    upload(1, [0, 0, 255, 255]);
    draw(&path, &[paint(0x80ff0000)], IDENTITY, [0, 0, 8, 8], 2);
    for p in read(1).chunks_exact(4) {
        near(p, [128, 0, 127, 255]);
    }
    upload(1, [0; 4]);
    let bounds = draw(
        &path,
        &[paint(0xff00ff00)],
        [0.5, 0., 0., 0.5, 2., 2.],
        [3, 3, 5, 5],
        0,
    );
    assert_eq!(bounds, [2., 2., 4., 4.]);
    let pixels = read(1);
    for y in 0..8 {
        for x in 0..8 {
            near(
                &pixels[(y * 8 + x) * 4..][..4],
                if (3..5).contains(&x) && (3..5).contains(&y) {
                    [0, 255, 0, 255]
                } else {
                    [0; 4]
                },
            );
        }
    }
    upload(1, [0; 4]);
    let gradient = DrawPaint {
        brush_type: 4,
        colors: [0xffff0000, 0xff0000ff],
        wrap: 4,
        coordinates: [0., 0., 8., 0.],
        ..paint(0)
    };
    draw(&path, &[gradient], IDENTITY, [0, 0, 8, 8], 0);
    let pixels = read(1);
    near(&pixels[..4], [239, 0, 16, 255]);
    near(&pixels[28..32], [16, 0, 239, 255]);
    upload(1, [0; 4]);
    let line = [
        PathElement {
            verb: 0,
            reserved: 0,
            points: [1., 4., 0., 0., 0., 0.],
        },
        PathElement {
            verb: 1,
            reserved: 0,
            points: [7., 4., 0., 0., 0., 0.],
        },
    ];
    let pen = DrawPaint {
        stroke: 1,
        width: 2.,
        ..paint(0xffffffff)
    };
    assert_eq!(
        draw(&line, &[pen], IDENTITY, [0, 0, 8, 8], 0),
        [1., 3., 6., 2.]
    );
    let pixels = read(1);
    near(&pixels[(3 * 8 + 4) * 4..][..4], [255; 4]);
    near(&pixels[(2 * 8 + 4) * 4..][..4], [0; 4]);
    upload(1, [0; 4]);
    upload(2, [120, 40, 200, 128]);
    let image = DrawPaint {
        brush_type: 2,
        image: 2,
        wrap: 4,
        coordinates: [0., 0., 8., 8.],
        ..paint(0)
    };
    draw(&path, &[image], IDENTITY, [0, 0, 8, 8], 0);
    for p in read(1).chunks_exact(4) {
        near(p, [120, 40, 200, 128]);
    }
    let in_place = DrawPaint { image: 1, ..image };
    draw(&path, &[in_place], IDENTITY, [0, 0, 8, 8], 0);
    for p in read(1).chunks_exact(4) {
        near(p, [120, 40, 200, 192]);
    }
    let original = read(1);
    draw(
        &path,
        &[paint(0xff123456)],
        IDENTITY,
        [100, 100, 200, 200],
        0,
    );
    assert_eq!(read(1), original);
    let mut pixels = [0u8, 0, 255, 255].repeat(64);
    for y in 2..6 {
        for x in 2..6 {
            pixels[(y * 8 + x) * 4..][..4].copy_from_slice(&[0, 255, 0, 255]);
        }
    }
    let mut c = command(2, 2);
    c.data = pixels.as_mut_ptr().cast();
    dispatch(c);
    upload(1, [0; 4]);
    let crop = DrawPaint {
        coordinates: [2., 2., 4., 4.],
        paint_transform: [2., 0., 0., 2., -4., -4.],
        ..image
    };
    draw(&path, &[crop], IDENTITY, [0, 0, 8, 8], 0);
    for p in read(1).chunks_exact(4) {
        near(p, [0, 255, 0, 255]);
    }
    for style in 0..53 {
        upload(1, [0; 4]);
        let hatch = DrawPaint {
            brush_type: 1,
            hatch: style,
            colors: [0xffff0000, 0xff0000ff],
            ..paint(0)
        };
        draw(&path, &[hatch], IDENTITY, [0, 0, 8, 8], 0);
        let pixels = read(1);
        let coverage=include_bytes!("../src/render/layer_ex_draw_hatch.bin");
        for (index,p) in pixels.chunks_exact(4).enumerate() {
            let value=coverage[style as usize*64+index];
            assert_eq!(p,[value,0,255-value,255],"hatch style {style} at pixel {index}");
        }
    }
    upload(1, [0; 4]);
    let points = [
        BrushPoint { x: 0., y: 0. },
        BrushPoint { x: 8., y: 0. },
        BrushPoint { x: 8., y: 8. },
        BrushPoint { x: 0., y: 8. },
    ];
    let radial = DrawPaint {
        brush_type: 3,
        colors: [0xffff0000, 0xff0000ff],
        center: [4., 4.],
        wrap: 4,
        brush_point_count: 4,
        brush_points: points.as_ptr(),
        ..paint(0)
    };
    draw(&path, &[radial], IDENTITY, [0, 0, 8, 8], 0);
    let pixels = read(1);
    near(&pixels[(4 * 8 + 4) * 4..][..4], [223, 0, 32, 255]);
    near(&pixels[(4 * 8 + 7) * 4..][..4], [32, 0, 223, 255]);
    upload(1, [0; 4]);
    let tiled = DrawPaint {
        brush_type: 4,
        colors: [0xffff0000, 0xff0000ff],
        coordinates: [0., 0., 4., 4.],
        tile: [0., 0., 4., 4.],
        wrap: 1,
        ..paint(0)
    };
    draw(&path, &[tiled], IDENTITY, [0, 0, 8, 8], 0);
    let pixels = read(1);
    near(&pixels[(1 * 8 + 1) * 4..][..4], [159, 0, 96, 255]);
    near(&pixels[(1 * 8 + 5) * 4..][..4], [127, 0, 128, 255]);
    upload(1, [0; 4]);
    let inset = DrawPaint {
        stroke: 1,
        width: 1.,
        alignment: 1,
        ..paint(0xffffffff)
    };
    draw(&path, &[inset], IDENTITY, [0, 0, 8, 8], 0);
    let pixels = read(1);
    near(&pixels[..4], [255; 4]);
    near(&pixels[(4 * 8 + 4) * 4..][..4], [0; 4]);
    upload(1, [0; 4]);
    let compound = [0., 0.25, 0.75, 1.];
    let bands = DrawPaint {
        stroke: 1,
        width: 4.,
        compound_count: 4,
        compound: compound.as_ptr(),
        ..paint(0xffffffff)
    };
    draw(&line, &[bands], IDENTITY, [0, 0, 8, 8], 0);
    let pixels = read(1);
    near(&pixels[(2 * 8 + 4) * 4..][..4], [255; 4]);
    near(&pixels[(3 * 8 + 4) * 4..][..4], [0; 4]);
    upload(1, [0; 4]);
    let arrow = DrawPaint {
        stroke: 1,
        width: 1.,
        end_cap: 255,
        end_arrow: [4., 3., 0., 1.],
        ..paint(0xffffffff)
    };
    draw(&line, &[arrow], IDENTITY, [0, 0, 8, 8], 0);
    let pixels = read(1);
    assert!(pixels[(3 * 8 + 5) * 4 + 3] > 128);
    near(&pixels[(1 * 8 + 5) * 4..][..4], [0; 4]);
    let mut resize = command(0, 1);
    resize.rect = [0, 0, 4, 4];
    dispatch(resize);
    draw(&path, &[paint(0xffffff00)], IDENTITY, [0, 0, 4, 4], 0);
    let pixels = krkr_plugin::render::read_pixels(1).unwrap();
    assert_eq!(pixels.len(), 64);
    for p in pixels.chunks_exact(4) {
        near(p, [255, 255, 0, 255]);
    }
    let mut resize = command(0, 1);
    resize.rect = [0, 0, 64, 64];
    dispatch(resize);
    let clear = || {
        let pixels = vec![0u8; 64 * 64 * 4];
        let mut c = command(2, 1);
        c.rect = [0, 0, 64, 64];
        c.pitch = 256;
        c.data = pixels.as_ptr().cast_mut().cast();
        dispatch(c);
    };
    let transformed_line = [
        PathElement { verb: 0, reserved: 0, points: [0.; 6] },
        PathElement { verb: 1, reserved: 0, points: [16., 0., 0., 0., 0., 0.] },
    ];
    clear();
    draw(&transformed_line, &[DrawPaint { stroke: 1, width: 0., ..paint(0xffffffff) }],
        [0., 2., -0.5, 0., 20.5, 5.], [19, 10, 22, 28], 0);
    let pixels = krkr_plugin::render::read_pixels(1).unwrap();
    for y in 0..64 { for x in 0..64 {
        near(&pixels[(y * 64 + x) * 4..][..4],
            if x == 20 && (10..28).contains(&y) { [255; 4] } else { [0; 4] });
    } }
    clear();
    let dashes = [2., 2.];
    draw(&transformed_line, &[DrawPaint { stroke: 1, width: 2., dash_count: 2,
        dashes: dashes.as_ptr(), ..paint(0xffffffff) }],
        [0., 2., -0.5, 0., 20.5, 5.], [19, 10, 22, 28], 0);
    let pixels = krkr_plugin::render::read_pixels(1).unwrap();
    for (x,y,expected) in [(20,11,[255;4]),(20,17,[0;4]),(20,25,[255;4]),(19,11,[0;4]),(20,29,[0;4])] {
        near(&pixels[(y * 64 + x) * 4..][..4],expected);
    }
    for id in [1, 2] {
        dispatch(command(1, id));
    }
}
