use bevy::{
    app::PluginsState,
    asset::RenderAssetUsages,
    camera::RenderTarget,
    prelude::*,
    render::{
        render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages},
        view::screenshot::{Screenshot, ScreenshotCaptured},
    },
    window::ExitCondition,
};
use bevy_krkr::{
    plugin::{host, render::KrkrRenderPlugin},
    prelude::KirikiriCamera,
    script::Tjs2Engine,
    sys,
};
use std::{
    ffi::CString,
    sync::mpsc,
    time::{Duration, Instant},
};

fn capture(app: &mut App, target: &Handle<Image>) -> [u8; 4] {
    for _ in 0..60 {
        app.update();
        std::thread::sleep(Duration::from_millis(10));
    }
    let (send, receive) = mpsc::channel();
    app.world_mut()
        .spawn(Screenshot::image(target.clone()))
        .observe(move |event: On<ScreenshotCaptured>| {
            let data = event.image.data.as_ref().unwrap();
            let i = (event.image.width() as usize * 16 + 16) * 4;
            send.send(<[u8; 4]>::try_from(&data[i..i + 4]).unwrap())
                .unwrap();
        });
    let start = Instant::now();
    loop {
        app.update();
        if let Ok(color) = receive.try_recv() {
            return color;
        }
        assert!(
            start.elapsed() < Duration::from_secs(15),
            "screenshot timed out"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn sprite_tracks_gpu_texture_updates() {
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
    app.add_plugins((bevy_krkr::kag::KagPlugin, KrkrRenderPlugin));
    while app.plugins_state() != PluginsState::Ready {
        bevy::tasks::tick_global_task_pools_on_main_thread();
    }
    app.finish();
    app.cleanup();
    let mut target = Image::new_uninit(
        Extent3d {
            width: 32,
            height: 32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    target.texture_descriptor.usage =
        TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_SRC;
    let target = app.world_mut().resource_mut::<Assets<Image>>().add(target);
    app.world_mut().spawn((
        Camera2d,
        KirikiriCamera,
        RenderTarget::Image(target.clone().into()),
    ));
    let path = CString::new(format!("{}/assets/game", env!("CARGO_MANIFEST_DIR"))).unwrap();
    assert_eq!(
        unsafe { sys::krkr_engine_init(path.as_ptr()) },
        sys::KRKR_OK
    );
    let engine = Tjs2Engine::new().unwrap();
    let screen = host::start(&engine, UVec2::splat(32)).unwrap();
    engine
        .exec_script(
            "krkrBevy.message.visible=false; krkrBevy.base.fillRect(0,0,32,32,0xffff0000);",
            "presentation",
        )
        .unwrap();
    app.world_mut().insert_non_send(screen);
    assert_eq!(capture(&mut app, &target), [255, 0, 0, 255]);
    engine
        .exec_script(
            "krkrBevy.base.fillRect(0,0,32,32,0xff00ff00);",
            "presentation",
        )
        .unwrap();
    assert_eq!(capture(&mut app, &target), [0, 255, 0, 255]);
    drop(app.world_mut().remove_non_send::<host::KrkrScreen>());
    host::stop(&engine);
    drop(engine);
    unsafe { sys::krkr_engine_shutdown() };
}
