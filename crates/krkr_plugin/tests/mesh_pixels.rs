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
        (3, [140, 140, 180, 255]),
    ] {
        upload(1, &source);
        upload(2, &background);
        let mesh = MeshCommand { blend, ..mesh };
        assert_eq!(unsafe { krkr_render_mesh(&mesh) }, 0);
        assert_color(read(2), expected);
    }

    for id in [1, 2, 3] {
        dispatch(command(1, id));
    }
}
