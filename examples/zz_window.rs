//! Temporary probe: presents the engine frame in a real window and captures it.
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use bevy_krkr::{plugin::host, prelude::*, script::Tjs2Engine, sys};
use std::ffi::CString;

fn main() {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "scanline probe".into(),
            resolution: (1280, 720).into(),
            ..default()
        }),
        ..default()
    }))
    .add_plugins(BevyKrkrPlugin {
        screen_size: UVec2::new(1920, 1080),
    });
    app.add_systems(Startup, setup);
    app.add_systems(Update, (drive, capture));
    app.add_observer(|captured: On<ScreenshotCaptured>| {
        let size = captured.image.size();
        let mut out = format!("P6\n{} {}\n255\n", size.x, size.y).into_bytes();
        for p in captured.image.data.as_ref().unwrap().chunks_exact(4) {
            out.extend_from_slice(&p[..3]);
        }
        std::fs::write("/tmp/krkr_probe/window.ppm", out).unwrap();
        println!("wrote window.ppm {size:?}");
        std::process::exit(0);
    });
    app.run();
}

fn setup(mut commands: Commands) {
    commands.spawn((Camera2d, KirikiriCamera));
}

fn drive(world: &mut World, mut ran: Local<bool>) {
    if *ran {
        return;
    }
    *ran = true;
    let Some(window) = world.query::<&Window>().iter(world).next().cloned() else {
        return;
    };
    let root =
        CString::new(format!("{}/test_games/extrans_showcase", env!("CARGO_MANIFEST_DIR"))).unwrap();
    unsafe { sys::krkr_engine_init(root.as_ptr()) };
    let engine = Tjs2Engine::new().unwrap();
    let screen = host::start(&engine, UVec2::new(1920, 1080)).unwrap();
    let run = |s: &str| engine.exec_script(s, "probe").unwrap();
    run(include_str!("../test_games/extrans_showcase/startup.tjs"));
    run("showcaseTimer.enabled=false;");
    run(r#"
        try { showcaseTo.stopTransition(); } catch(e) {}
        showcaseFrom.visible=false; showcaseTo.visible=true;
        global.freeze=0;
        global.options=showcaseOptions["scanline"];
        options.time=1000; options.selfupdate=true;
        options.callback=function(){return freeze;};
        showcaseTo.beginTransition("scanline",false,showcaseFrom,options);
        "#);
    run("freeze=500;");
    println!(
        "window physical {}x{} logical {:.0}x{:.0} scale {}",
        window.resolution.physical_width(),
        window.resolution.physical_height(),
        window.resolution.width(),
        window.resolution.height(),
        window.scale_factor()
    );
    world.insert_non_send(screen);
    world.insert_non_send(engine);
}

fn capture(mut commands: Commands, mut frames: Local<u32>) {
    *frames += 1;
    if *frames == 30 {
        commands.spawn(Screenshot::primary_window());
    }
}
