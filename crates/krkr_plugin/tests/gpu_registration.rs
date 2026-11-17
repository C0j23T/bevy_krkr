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

fn register(owner: &std::ffi::CStr, name: &std::ffi::CStr, source: &str) -> (i32, u32) {
    let source = std::ffi::CString::new(source).unwrap();
    let mut opcode = 0;
    let code = unsafe {
        krkr_shim::render::krkr_render_register_method(
            owner.as_ptr(),
            name.as_ptr(),
            source.as_ptr(),
            &mut opcode,
        )
    };
    (code, opcode)
}

fn read(id: u64) -> Vec<u8> {
    let mut pixels = vec![0; 24];
    let mut request = command(3, id);
    request.data = pixels.as_mut_ptr().cast();
    dispatch(request);
    pixels
}

#[test]
fn registered_wesl_methods_compile_and_roll_back_atomically() {
    initialize_renderer();
    let source = r#"
        import package::krkr::operations::{p, source, coordinate, sample};
        public fn render(at: vec2<f32>, xy: vec2<i32>) -> vec4<f32> {
            let color = sample(source, coordinate(0u, at));
            return vec4(color.bg, p[2].y, color.a);
        }
    "#;
    let (code, first) = register(c"first-owner", c"GpuFirst", source);
    assert_eq!(code, 0);
    let (code, second) = register(c"second-owner", c"GpuSecond", source);
    assert_eq!(code, 0);
    assert!(first > 100 && second > 100);
    assert_ne!(first, second);
    assert_ne!(register(c"second-owner", c"GpuFirst", source).0, 0);
    assert_ne!(register(c"first-owner", c"Copy", source).0, 0);
    assert_ne!(register(c"first-owner", c"LayerExImageGaussian", source).0, 0);
    assert_ne!(register(c"first-owner", c"BadShader", "invalid WESL").0, 0);
    assert_ne!(
        register(
            c"first-owner",
            c"BadShader",
            r#"
        public fn render(at: vec2<f32>, xy: vec2<i32>) -> vec3<f32> { return vec3(0.0); }
    "#
        )
        .0,
        0
    );
    for id in [1, 2] {
        dispatch(command(0, id));
    }
    let pixels = [10u8, 20, 30, 40].repeat(6);
    let mut upload = command(2, 1);
    upload.data = pixels.as_ptr().cast_mut().cast();
    dispatch(upload);
    let mut draw = command(4, 2);
    draw.method = c"GpuFirst".as_ptr();
    draw.sources[0] = 1;
    draw.source_rects[0] = [0.0, 0.0, 3.0, 2.0];
    draw.parameters[1] = 100.0 / 255.0;
    dispatch(draw);
    assert_eq!(read(2), [30u8, 20, 100, 40].repeat(6));
    assert_eq!(
        unsafe { krkr_shim::render::krkr_render_unregister_owner(c"first-owner".as_ptr()) },
        0
    );
    assert_ne!(unsafe { krkr_render_dispatch(&mut draw) }, 0);
    draw.method = c"GpuSecond".as_ptr();
    dispatch(draw);
    assert_eq!(read(2), [30u8, 20, 100, 40].repeat(6));
    let (code, third) = register(c"first-owner", c"GpuFirst", source);
    assert_eq!(code, 0);
    assert_ne!(third, first);
    assert_ne!(third, second);
    for owner in [c"first-owner", c"second-owner"] {
        assert_eq!(
            unsafe { krkr_shim::render::krkr_render_unregister_owner(owner.as_ptr()) },
            0
        );
    }
    for id in [1, 2] {
        dispatch(command(1, id));
    }
}
