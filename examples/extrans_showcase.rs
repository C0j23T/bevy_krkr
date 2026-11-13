use bevy::prelude::*;
use bevy_krkr::{prelude::*, script::Tjs2Engine};
use std::path::PathBuf;

#[derive(Resource)]
struct ExternalPlugin {
    path: PathBuf,
    once: bool,
}

fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let once = args.iter().any(|arg| arg == "--once");
    let path = args
        .into_iter()
        .find(|arg| arg != "--once")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").expect("HOME"))
                .join("Workspace/extrans/extrans.so")
        });
    let path = path
        .canonicalize()
        .expect("build extrans.so with tools/extrans/build.sh");
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "external extrans.so showcase".into(),
                resolution: (1280, 720).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(BevyKrkrPlugin {
            screen_size: UVec2::new(1920, 1080),
        })
        .insert_resource(ExternalPlugin { path, once })
        .add_systems(Startup, setup)
        .add_systems(Update, start_game)
        .run();
}

fn setup(mut commands: Commands, mut enter: MessageWriter<KirikiriEnterEvent>) {
    commands.spawn((Camera2d, KirikiriCamera));
    enter.write(KirikiriEnterEvent {
        data_xp3_path: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("test_games/extrans_showcase"),
    });
}

fn start_game(
    plugin: Res<ExternalPlugin>,
    engine: Option<NonSend<Tjs2Engine>>,
    mut started: Local<bool>,
    mut exit: MessageWriter<AppExit>,
) {
    let Some(engine) = engine else { return };
    if *started {
        if plugin.once && engine.eval("showcaseCompleted >= 7").unwrap() == "1" {
            exit.write(AppExit::Success);
        }
        return;
    }
    let path = plugin.path.to_str().expect("UTF-8 plugin path");
    engine
        .exec_script(&format!("Plugins.link({path:?});"), "external plugin")
        .expect("load external extrans.so");
    engine
        .exec_script(
            include_str!("extrans_showcase.tjs"),
            "external extrans showcase",
        )
        .expect("run external extrans showcase");
    *started = true;
}
