use bevy::{
    app::AppExit,
    prelude::*,
    render::view::Msaa,
    window::{PresentMode, WindowResolution},
    winit::WinitSettings,
};
use bevy_krkr::{plugin::render::KirikiriScene, prelude::*, script::Tjs2Engine};

const WIDTH: u32 = 800;
const HEIGHT: u32 = 600;

fn main() {
    let krkr = std::env::args().any(|arg| arg == "--krkr");
    let measure = std::env::args().any(|arg| arg == "--measure");

    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: if krkr { "Kirikiri idle" } else { "Bevy idle" }.into(),
            present_mode: PresentMode::AutoNoVsync,
            resolution: WindowResolution::new(WIDTH, HEIGHT).with_scale_factor_override(1.0),
            ..default()
        }),
        ..default()
    }))
    .insert_resource(ClearColor(Color::BLACK))
    .insert_resource(WinitSettings::continuous());

    if krkr {
        app.add_plugins(BevyKrkrPlugin {
            screen_size: UVec2::new(WIDTH, HEIGHT),
        })
        .add_systems(Startup, setup_krkr)
        .add_systems(Update, draw_static_scene);
    } else {
        app.add_systems(Startup, setup_bevy);
    }

    if measure {
        app.add_systems(Last, measure_idle);
    }
    app.run();
}

fn setup_bevy(mut commands: Commands) {
    commands.spawn((Camera2d, Msaa::Off));
    commands.spawn(Sprite::from_color(Color::WHITE, Vec2::splat(200.0)));
}

fn setup_krkr(mut commands: Commands, mut enter: MessageWriter<KirikiriEnterEvent>) {
    commands.spawn((Camera2d, Msaa::Off, KirikiriCamera));
    enter.write(KirikiriEnterEvent {
        data_xp3_path: concat!(env!("CARGO_MANIFEST_DIR"), "/assets/game").into(),
    });
}

fn draw_static_scene(engine: Option<NonSend<Tjs2Engine>>, mut drawn: Local<bool>) {
    if *drawn {
        return;
    }
    let Some(engine) = engine else {
        return;
    };
    let script = format!(
        r#"
        global.benchmarkWindow = new Window();
        benchmarkWindow.setInnerSize({WIDTH},{HEIGHT});
        global.benchmarkLayer = new Layer(benchmarkWindow,null);
        benchmarkLayer.setImageSize({WIDTH},{HEIGHT});
        benchmarkLayer.setSizeToImageSize();
        benchmarkLayer.fillRect(0,0,{WIDTH},{HEIGHT},0xff000000);
        benchmarkLayer.fillRect({left},{top},200,200,0xffffffff);
        benchmarkWindow.visible = true;
        "#,
        left = (WIDTH - 200) / 2,
        top = (HEIGHT - 200) / 2,
    );
    engine.exec_script(&script, "Idle benchmark").unwrap();
    *drawn = true;
}

#[derive(Default)]
struct Sample {
    ready_at: Option<f64>,
    start: Option<f64>,
    frames: u64,
    #[cfg(feature = "test-render-stats")]
    render_stats: Option<bevy_krkr::plugin::render::RenderStats>,
}

fn measure_idle(
    time: Res<Time<Real>>,
    scene: Option<Res<KirikiriScene>>,
    mut sample: Local<Sample>,
    mut exit: MessageWriter<AppExit>,
) {
    let now = time.elapsed_secs_f64();
    if scene
        .as_ref()
        .is_some_and(|scene| scene.size == UVec2::ZERO)
    {
        if now > 10.0 {
            error!("Kirikiri scene did not become visible");
            exit.write(AppExit::error());
        }
        return;
    }
    let ready_at = *sample.ready_at.get_or_insert(now);
    if now - ready_at < 2.0 {
        return;
    }
    if let Some(start) = sample.start {
        sample.frames += 1;
        let seconds = now - start;
        if seconds >= 5.0 {
            let label = if scene.is_some() { "Kirikiri" } else { "Bevy" };
            println!(
                "{label} idle: {:.1} FPS ({} frames in {seconds:.2} s)",
                sample.frames as f64 / seconds,
                sample.frames
            );
            #[cfg(feature = "test-render-stats")]
            if let Some(start) = sample.render_stats {
                let end = bevy_krkr::plugin::render::stats().unwrap();
                println!("Kirikiri render stats at start: {start:?}");
                println!("Kirikiri render stats at end:   {end:?}");
                assert_eq!(start, end);
            }
            exit.write(AppExit::Success);
        }
    } else {
        sample.start = Some(now);
        #[cfg(feature = "test-render-stats")]
        {
            sample.render_stats = bevy_krkr::plugin::render::stats();
        }
    }
}
