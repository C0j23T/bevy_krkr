use bevy::{
    app::PluginsState,
    prelude::*,
    render::renderer::{RenderDevice, RenderQueue},
    window::ExitCondition,
};
use krkr_shim::render::{OP_COPY_TO_EXISTING_TEXTURE, RenderCommand, krkr_render_dispatch};

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

fn command(operation: u32, target: u64, width: i32, height: i32) -> RenderCommand {
    RenderCommand {
        operation,
        format: 4,
        target,
        rect: [0, 0, width, height],
        pitch: width * 4,
        ..unsafe { std::mem::zeroed() }
    }
}

fn dispatch(mut command: RenderCommand) {
    assert_eq!(unsafe { krkr_render_dispatch(&mut command) }, 0);
}

fn upload(id: u64, width: i32, height: i32, pixels: &[u8]) {
    let mut upload = command(2, id, width, height);
    upload.data = pixels.as_ptr().cast_mut().cast();
    dispatch(upload);
}

fn read(id: u64, width: i32, height: i32) -> Vec<u8> {
    let mut pixels = vec![0; (width * height * 4) as usize];
    let mut read = command(3, id, width, height);
    read.data = pixels.as_mut_ptr().cast();
    dispatch(read);
    pixels
}

#[test]
fn copy_to_existing_texture_copies_region_and_preserves_other_pixels() {
    initialize_renderer();
    dispatch(command(0, 1, 2, 2));
    dispatch(command(0, 2, 2, 2));
    dispatch(command(0, 3, 1, 1));

    let source = [
        255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 0, 255,
    ];
    upload(1, 2, 2, &source);
    let background = [11, 22, 33, 44].repeat(4);
    upload(2, 2, 2, &background);

    let mut composite = command(5, 2, 2, 2);
    composite.sources[0] = 1;
    composite.source_rects[0] = [1.0, 0.0, 2.0, 1.0];
    composite.rect = [0, 1, 1, 2];
    composite.parameters[0] = 255.0;
    dispatch(composite);
    assert_eq!(
        read(2, 2, 2),
        [
            11, 22, 33, 44, 11, 22, 33, 44, 0, 255, 0, 255, 11, 22, 33, 44,
        ]
    );

    let mut copy = command(OP_COPY_TO_EXISTING_TEXTURE, 2, 2, 2);
    copy.sources[0] = 1;
    copy.source_rects[0] = [1.0, 0.0, 2.0, 1.0];
    copy.rect = [0, 1, 1, 2];
    dispatch(copy);

    assert_eq!(
        read(2, 2, 2),
        [
            11, 22, 33, 44, 11, 22, 33, 44, 0, 255, 0, 255, 11, 22, 33, 44,
        ]
    );

    copy.source_rects[0] = [1.0, 0.0, 2.0, 2.0];
    assert_ne!(unsafe { krkr_render_dispatch(&mut copy) }, 0);
    copy.source_rects[0] = [-1.0, 0.0, 0.0, 1.0];
    copy.rect = [0, 0, 1, 1];
    assert_ne!(unsafe { krkr_render_dispatch(&mut copy) }, 0);
    copy.source_rects[0] = [0.0, 0.0, 1.0, 1.0];
    copy.rect = [i32::MIN, 0, i32::MAX, 1];
    assert_ne!(unsafe { krkr_render_dispatch(&mut copy) }, 0);
    copy.sources[0] = 2;
    copy.source_rects[0] = [0.0, 0.0, 1.0, 1.0];
    copy.rect = [0, 0, 1, 1];
    assert_ne!(unsafe { krkr_render_dispatch(&mut copy) }, 0);

    for id in [1, 2, 3] {
        dispatch(command(1, id, 1, 1));
    }
}
