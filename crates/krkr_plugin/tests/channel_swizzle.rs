use bevy::{
    app::PluginsState,
    prelude::*,
    render::renderer::{RenderDevice, RenderQueue},
    window::ExitCondition,
};
use krkr_shim::render::{RenderCommand, krkr_render_dispatch};

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
        target,
        format: 4,
        rect: [0, 0, 3, 2],
        pitch: 12,
        ..unsafe { std::mem::zeroed() }
    }
}

fn dispatch(mut command: RenderCommand) {
    assert_eq!(unsafe { krkr_render_dispatch(&mut command) }, 0);
}

#[test]
fn gpu_channel_arrays_preserve_pixels_and_clip() {
    initialize_renderer();
    let source: Vec<u8> = (0..6)
        .flat_map(|i| [10 + i, 50 + i, 100 + i, 200 + i])
        .collect();
    for id in [1, 2] {
        dispatch(command(0, id));
    }
    let mut upload = command(2, 1);
    upload.data = source.as_ptr().cast_mut().cast();
    dispatch(upload);

    for channels in [
        [0, 1, 2, 3],
        [2, 1, 0, 3],
        [3, 0, 1, 2],
        [1, 1, 1, 0],
        [3, 2, 1, 0],
    ] {
        let mut copy = command(4, 2);
        copy.method = c"ChannelSwizzle".as_ptr();
        copy.sources[0] = 1;
        copy.source_rects[0] = [0., 0., 3., 2.];
        copy.parameters[..4].copy_from_slice(&channels.map(|c| c as f32));
        dispatch(copy);
        let expected: Vec<u8> = source
            .chunks_exact(4)
            .flat_map(|pixel| channels.map(|c| pixel[c]))
            .collect();
        let mut actual = vec![0; source.len()];
        let mut read = command(3, 2);
        read.data = actual.as_mut_ptr().cast();
        dispatch(read);
        assert_eq!(actual, expected, "channels {channels:?}");
    }

    let mut in_place = command(4, 1);
    in_place.method = c"ChannelSwizzle".as_ptr();
    in_place.sources[0] = 1;
    in_place.rect = [1, 0, 3, 1];
    in_place.source_rects[0] = [1., 0., 3., 1.];
    in_place.parameters[..4].copy_from_slice(&[2., 1., 0., 3.]);
    dispatch(in_place);
    let mut actual = vec![0; source.len()];
    let mut read = command(3, 1);
    read.data = actual.as_mut_ptr().cast();
    dispatch(read);
    let mut expected = source;
    expected[4..12].chunks_exact_mut(4).for_each(|p| p.swap(0, 2));
    assert_eq!(actual, expected);
    for id in [1, 2] {
        dispatch(command(1, id));
    }
}
