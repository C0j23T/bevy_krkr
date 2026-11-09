//! Minimal KAG example: one line of text and a looping BGM.
//!
//! `assets/game/first.ks` is parsed by Kirikiroid2's KAGParser through
//! `krkr_sys`; this program only reacts to the resulting messages. The text is
//! laid out and rasterised by Kirikiroid2 into its message-window layer, which
//! `krkr_plugin::render` draws as a sprite. The BGM is played through
//! `bevy_kira_audio`. Click (or press Enter / Space) to advance.
//!
//! ```sh
//! cargo run --example minimal_kag [-- path/to/data.xp3]
//! ```
//!
//! Needs the Kirikiroid2 checkout (`./Kirikiroid2` or `$KIRIKIROID2_SRC`,
//! branch `bevy-port`); see README.md. Without one, krkr_sys builds stub
//! bindings and the program panics when the instance starts.

use bevy::prelude::*;
use bevy_krkr::prelude::*;

fn main() {
    let data = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "assets/game".into());
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "bevy_krkr – minimal KAG".into(),
                resolution: (800, 600).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(BevyKrkrPlugin::default())
        .insert_resource(DataPath(data))
        .add_systems(Startup, setup)
        .add_systems(Update, (log_kag, exit_when_done))
        .run();
}

#[derive(Resource)]
struct DataPath(String);

fn setup(
    mut commands: Commands,
    data: Res<DataPath>,
    mut enter: MessageWriter<KirikiriEnterEvent>,
) {
    // bevy_krkr never spawns a camera; mark one of yours.
    commands.spawn((Camera2d, KirikiriCamera));
    enter.write(KirikiriEnterEvent {
        data_xp3_path: data.0.clone().into(),
    });
}

fn log_kag(mut text: MessageReader<ShowMessageEvent>, mut bgm: MessageReader<PlayBgmEvent>) {
    for m in text.read() {
        info!("message: {}", m.text);
    }
    for b in bgm.read() {
        info!("bgm: {}", b.storage);
    }
}

/// Close the window when the scenario ends or the engine asks to terminate.
fn exit_when_done(
    mut ended: MessageReader<KagScriptEndEvent>,
    mut exited: MessageReader<KirikiriExitEvent>,
    mut app_exit: MessageWriter<AppExit>,
) {
    if ended.read().count() + exited.read().count() > 0 {
        app_exit.write(AppExit::Success);
    }
}
