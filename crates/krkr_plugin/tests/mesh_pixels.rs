use bevy::{
    app::PluginsState,
    prelude::*,
    render::renderer::{RenderDevice, RenderQueue},
    window::ExitCondition,
};
use krkr_shim::{
    mesh::{MeshCommand, MeshSurface, krkr_render_mesh},
    render::{RenderCommand, krkr_render_dispatch},
};

fn initialize_renderer() {
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

fn command(operation: u32, target: u64) -> RenderCommand {
    RenderCommand {
        operation,
        format: 4,
        target,
        rect: [0, 0, 2, 2],
        pitch: 8,
        ..unsafe { std::mem::zeroed() }
    }
}

fn dispatch(mut command: RenderCommand) {
    assert_eq!(unsafe { krkr_render_dispatch(&mut command) }, 0);
}

fn upload(id: u64, pixels: &[u8; 16]) {
    let mut upload = command(2, id);
    upload.data = pixels.as_ptr().cast_mut().cast();
    dispatch(upload);
}

fn read(id: u64) -> [u8; 16] {
    let mut pixels = [0; 16];
    let mut read = command(3, id);
    read.data = pixels.as_mut_ptr().cast();
    dispatch(read);
    pixels
}

fn assert_color(pixels: [u8; 16], expected: [u8; 4]) {
    for pixel in pixels.chunks_exact(4) {
        assert!(
            pixel
                .iter()
                .zip(expected)
                .all(|(&actual, expected)| actual.abs_diff(expected) <= 1),
            "{pixel:?} != {expected:?}"
        );
    }
}

#[test]
fn krkr_render_mesh_deforms_rasterizes_and_updates_target_pixels() {
    initialize_renderer();
    for id in [1, 2, 3] {
        dispatch(command(0, id));
    }
    let source = [
        255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 0, 255,
    ];
    upload(1, &source);

    let mut surface: MeshSurface = unsafe { std::mem::zeroed() };
    surface.models[31][0] = 1.0;
    surface.models[31][5] = 1.0;
    surface.models[31][10] = 1.0;
    surface.models[31][15] = 1.0;
    surface.geometry = [1.0, 1.0, 2.0, 2.0];
    let mesh = MeshCommand {
        target: 2,
        source: 1,
        mask: 0,
        surfaces: &surface,
        surface_count: 1,
        division_x: 1,
        division_y: 1,
        blend: 0,
        opacity: 1.0,
        tint: [1.0; 4],
        color: [0.0; 4],
    };
    assert_eq!(unsafe { krkr_render_mesh(&mesh) }, 0);
    assert_eq!(read(2), source);

    let source = [255, 0, 0, 255].repeat(4).try_into().unwrap();
    let background = [0, 0, 255, 255].repeat(4).try_into().unwrap();
    upload(1, &source);
    upload(2, &background);
    let half_tinted = MeshCommand {
        opacity: 0.5,
        tint: [0.5, 1.0, 1.0, 1.0],
        ..mesh
    };
    assert_eq!(unsafe { krkr_render_mesh(&half_tinted) }, 0);
    assert_color(read(2), [64, 0, 128, 255]);

    let mode21 = MeshCommand {
        blend: 21,
        color: [0.0, 1.0, 0.0, 0.5],
        ..mesh
    };
    upload(2, &background);
    assert_eq!(unsafe { krkr_render_mesh(&mode21) }, 0);
    assert_color(read(2), [0, 128, 128, 255]);

    let transparent_mask = [0, 0, 0, 0].repeat(4).try_into().unwrap();
    upload(3, &transparent_mask);
    let masked = MeshCommand { mask: 3, ..mesh };
    upload(2, &background);
    assert_eq!(unsafe { krkr_render_mesh(&masked) }, 0);
    assert_eq!(read(2), background);

    let opaque_mask = [0, 0, 0, 255].repeat(4).try_into().unwrap();
    upload(3, &opaque_mask);
    upload(2, &background);
    assert_eq!(unsafe { krkr_render_mesh(&masked) }, 0);
    assert_eq!(read(2), source);

    upload(3, &[0, 0, 0, 255, 0, 0, 0, 0, 0, 0, 0, 255, 0, 0, 0, 0]);
    upload(2, &background);
    assert_eq!(unsafe { krkr_render_mesh(&masked) }, 0);
    assert_eq!(
        read(2),
        [255, 0, 0, 255, 0, 0, 255, 255].repeat(2).as_slice()
    );

    let mut bent = surface;
    bent.kind = 1;
    for row in 0..4 {
        for col in 0..4 {
            let point = row * 8 + col * 2;
            bent.control_points[point] = 1.0 - col as f32 / 3.0;
            bent.control_points[point + 1] = row as f32 / 3.0;
        }
    }
    let flipped = MeshCommand {
        surfaces: &bent,
        ..mesh
    };
    upload(
        1,
        &[
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 0, 255,
        ],
    );
    upload(2, &background);
    assert_eq!(unsafe { krkr_render_mesh(&flipped) }, 0);
    assert_eq!(
        read(2),
        [
            0, 255, 0, 255, 255, 0, 0, 255, 255, 255, 0, 255, 0, 0, 255, 255
        ]
    );

    let original = read(1);
    let in_place = MeshCommand { target: 1, ..mesh };
    assert_eq!(unsafe { krkr_render_mesh(&in_place) }, 0);
    assert_eq!(read(1), original);

    for row in 0..4 {
        for col in 0..4 {
            let point = row * 8 + col * 2;
            bent.control_points[point] = if col == 3 { 0.5 } else { 0.0 };
            bent.control_points[point + 1] = row as f32 / 3.0;
        }
    }
    let curved = MeshCommand {
        surfaces: &bent,
        division_x: 4,
        division_y: 4,
        ..mesh
    };
    upload(
        1,
        &[
            255, 0, 0, 255, 0, 255, 0, 255, 255, 0, 0, 255, 0, 255, 0, 255,
        ],
    );
    upload(2, &background);
    assert_eq!(unsafe { krkr_render_mesh(&curved) }, 0);
    assert_eq!(
        read(2),
        [0, 255, 0, 255, 0, 0, 255, 255].repeat(2).as_slice()
    );

    let source = [200, 80, 40, 128].repeat(4).try_into().unwrap();
    let background = [40, 100, 160, 255].repeat(4).try_into().unwrap();
    for (blend, expected) in [
        (0, [120, 90, 100, 255]),
        (1, [36, 66, 92, 255]),
        (4, [36, 66, 92, 255]),
        (6, [40, 100, 160, 255]),
        (3, [140, 140, 180, 255]),
    ] {
        upload(1, &source);
        upload(2, &background);
        let mesh = MeshCommand { blend, ..mesh };
        assert_eq!(unsafe { krkr_render_mesh(&mesh) }, 0);
        assert_color(read(2), expected);
    }

    let untouched = [
        7, 19, 31, 0, 43, 59, 71, 64, 83, 97, 109, 128, 127, 139, 151, 255,
    ];
    upload(2, &untouched);
    let skipped = MeshCommand { blend: 6, ..mesh };
    assert_eq!(unsafe { krkr_render_mesh(&skipped) }, 0);
    assert_eq!(read(2), untouched);

    verify_surface_inheritance();

    for id in [1, 2, 3] {
        dispatch(command(1, id));
    }
}

fn affine(mask: u32, point: [f32; 2]) -> [f32; 2] {
    let [mut x, mut y] = point;
    if mask & 0x10 != 0 {
        (x, y) = (-y, x);
    }
    if mask & 0x20 != 0 {
        x *= 1.25;
    }
    if mask & 0x40 != 0 {
        y *= 0.75;
    }
    [
        x + if mask & 0x80 != 0 { 0.2 * y } else { 0.0 },
        y - if mask & 0x100 != 0 { 0.1 * x } else { 0.0 },
    ]
}

fn warp_scale(mask: u32, [x, y]: [f32; 2]) -> [f32; 2] {
    [
        x * if mask & 0x20 != 0 { 1.5 } else { 1.0 },
        y * if mask & 0x40 != 0 { 0.5 } else { 1.0 },
    ]
}

fn matrix(transform: impl Fn([f32; 2]) -> [f32; 2]) -> [f32; 16] {
    let x = transform([1.0, 0.0]);
    let y = transform([0.0, 1.0]);
    [
        x[0], x[1], 0.0, 0.0, y[0], y[1], 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

fn inheritance_fixture(mask: u32) -> [MeshSurface; 4] {
    let mut chain: [MeshSurface; 4] = unsafe { std::mem::zeroed() };
    chain[0].models[31] = matrix(|[x, y]| [x / 20.0, y / 20.0]);
    chain[0].placement = [-0.1, 0.08, 0.0, 0.0];
    chain[0].kind = 3;
    chain[1].kind = 2;
    chain[1].geometry = [100.0, -200.0, 90.0, 80.0];
    chain[1].offset = [200.0, -300.0, 0.0, 0.0];
    chain[1].placement = [-3.0, 2.0, 0.0, 0.0];
    chain[2].kind = 1;
    chain[2].geometry = [10.0, 8.0, 20.0, 16.0];
    chain[2].offset = [0.75, -0.25, 0.0, 0.0];
    chain[2].placement = [1.25, -1.5, 0.0, 0.0];
    chain[2].inherit_mask = 0x1b0;
    for row in 0..4 {
        for col in 0..4 {
            let at = row * 8 + col * 2;
            chain[2].control_points[at] =
                col as f32 / 3.0 + if row == 1 || row == 2 { 0.1 } else { 0.0 };
            chain[2].control_points[at + 1] =
                row as f32 / 3.0 + if col == 1 || col == 2 { 0.2 / 3.0 } else { 0.0 };
        }
    }
    chain[3].kind = 2;
    chain[3].geometry = [4.0, 3.0, 8.0, 6.0];
    chain[3].offset = [1.0, -0.5, 0.0, 0.0];
    chain[3].placement = [2.0, 1.0, 0.0, 0.0];
    chain[3].inherit_mask = mask;
    chain[3].models[31] = matrix(|[x, y]| [1.25 * x, 0.75 * y]);
    for variant in 0..32 {
        chain[1].models[variant] = matrix(|p| affine((variant as u32) << 4, p));
        chain[2].models[variant] = matrix(|p| warp_scale((variant as u32) << 4, p));
    }
    chain
}

// Closed form of this fixture, independent of the evaluator's contribution walk.
// The patch is x'=x+6*t*(1-t)-.75, y'=y+3.2*s*(1-s)+.25.
// Geometry inherits the leaf flags, leaf placement starts at warp, and warp
// placement starts at affine. Root projection always applies in full.
fn inheritance_oracle(mask: u32, u: f32, v: f32) -> [f32; 2] {
    let geometry = [10.0 * v - 6.25, 4.5 * u - 1.875];
    let t = (geometry[1] + 1.0 + 8.0) / 16.0;
    let s = (geometry[0] + 2.0 + 10.0) / 20.0;
    let deformed = [
        geometry[0] + 6.0 * t * (1.0 - t) - 0.75,
        geometry[1] + 3.2 * s * (1.0 - s) + 0.25,
    ];
    let g = affine(mask & 0x1b0, warp_scale(mask, deformed));
    let leaf_placement = affine(0x1b0, [3.0, 0.5]);
    let warp_placement = affine(0x1f0, [1.25, -1.5]);
    [
        (g[0] + leaf_placement[0] + warp_placement[0] - 3.0) / 20.0 - 0.1,
        (g[1] + leaf_placement[1] + warp_placement[1] + 2.0) / 20.0 + 0.08,
    ]
}

fn verify_surface_inheritance() {
    // Generated by fixtures/emote_surface_oracle.cpp against the C++ evaluator.
    let cpp_golden = [
        (0x0, 0.0, 0.0, [-0.424022228, 0.322150022]),
        (0x0, 0.375, 0.625, [-0.111398622, 0.412149996]),
        (0x0, 1.0, 1.0, [0.061476000, 0.541150093]),
        (0x10, 0.375, 0.625, [-0.189649999, 0.407351404]),
        (0x20, 0.375, 0.625, [-0.079263046, 0.412149996]),
        (0x40, 0.375, 0.625, [-0.111398622, 0.391387492]),
        (0x80, 0.375, 0.625, [-0.103093632, 0.412149996]),
        (0x100, 0.375, 0.625, [-0.111398622, 0.408477336]),
        (0x1f0, 0.0, 0.0, [-0.200597286, -0.046250544]),
        (0x1f0, 0.375, 0.625, [-0.163060218, 0.428309888]),
        (0x1f0, 1.0, 1.0, [-0.191822886, 0.695684373]),
        (0x20007fc, 0.375, 0.625, [-0.163060218, 0.428309888]),
    ];
    for (mask, u, v, expected) in cpp_golden {
        let actual = inheritance_oracle(mask, u, v);
        for (actual, expected) in actual.into_iter().zip(expected) {
            assert!(
                (actual - expected).abs() < 0.000001,
                "C++ evaluator mask={mask:#x}, uv=({u},{v}): {actual} != {expected}"
            );
        }
    }
    let size = 64;
    let mut create = command(0, 4);
    create.rect = [0, 0, size, size];
    dispatch(create);
    upload(1, &[255; 16]);
    for mask in [0, 0x10, 0x20, 0x40, 0x80, 0x100, 0x1f0, 0x20007fc] {
        let chain = inheritance_fixture(mask);
        let mesh = MeshCommand {
            target: 4,
            source: 1,
            mask: 0,
            surfaces: chain.as_ptr(),
            surface_count: 4,
            division_x: 8,
            division_y: 8,
            blend: 0,
            opacity: 1.0,
            tint: [1.0; 4],
            color: [0.0; 4],
        };
        let mut pixels = vec![0u8; (size * size * 4) as usize];
        let mut transfer = command(2, 4);
        transfer.rect = [0, 0, size, size];
        transfer.pitch = size * 4;
        transfer.data = pixels.as_mut_ptr().cast();
        dispatch(transfer);
        assert_eq!(unsafe { krkr_render_mesh(&mesh) }, 0);
        transfer.operation = 3;
        dispatch(transfer);
        let vertices: Vec<_> = (0..=8)
            .flat_map(|y| {
                (0..=8).map(move |x| {
                    let p = inheritance_oracle(mask, x as f32 / 8.0, y as f32 / 8.0);
                    [
                        (p[0] + 1.0) * size as f32 / 2.0,
                        (p[1] + 1.0) * size as f32 / 2.0,
                    ]
                })
            })
            .collect();
        let mut inside_count = 0;
        let mut outside_count = 0;
        for y in 0..size {
            for x in 0..size {
                let p = [x as f32 + 0.5, y as f32 + 0.5];
                let mut inside = false;
                let mut near_edge = false;
                for row in 0..8 {
                    for col in 0..8 {
                        let a = row * 9 + col;
                        for triangle in [[a, a + 1, a + 9], [a + 1, a + 10, a + 9]] {
                            let [a, b, c] = triangle.map(|i| vertices[i]);
                            let edge = |a: [f32; 2], b: [f32; 2]| {
                                (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])
                            };
                            let e = [edge(a, b), edge(b, c), edge(c, a)];
                            if e.iter().all(|v| *v >= 0.0) || e.iter().all(|v| *v <= 0.0) {
                                inside = true;
                            }
                            for (a, b, e) in [(a, b, e[0]), (b, c, e[1]), (c, a, e[2])] {
                                let length = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
                                near_edge |= e.abs() < length * 0.002;
                            }
                        }
                    }
                }
                if near_edge {
                    continue;
                }
                let pixel = &pixels[((y * size + x) * 4) as usize..][..4];
                assert_eq!(
                    pixel,
                    if inside { &[255; 4] } else { &[0; 4] },
                    "inherit mask={mask:#x}, pixel=({x},{y})"
                );
                if inside {
                    inside_count += 1;
                } else {
                    outside_count += 1;
                }
            }
        }
        assert!(inside_count > 20 && outside_count > 1000, "mask={mask:#x}");
    }
    dispatch(command(1, 4));
}
