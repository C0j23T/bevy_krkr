//! Start a retail game archive (`snbk/data.xp3`) through the bevy_krkr host.
//!
//! ```sh
//! cargo run --example snbk -- [data.xp3] [--dump-log PATH]
//! ```
//!
//! Defaults: `snbk/data.xp3`. Every archive next to the data file is mounted
//! and the game's own start-up scripts are run; the screen is drawn by the
//! game through Kirikiroid2, whose layers the host draws itself.
//!
//! What this archive needs on top of the plain engine:
//! - the game keeps its voice, CG and event images in **7z** archives next to
//!   `data.xp3` (`voice.xp3`, `fgimage720.xp3`, …), not in XP3. They are opened
//!   through libarchive (`Kirikiroid2/src/core/environ/bevy/Archives.cpp`),
//!   and this example mounts every archive next to the data file so the
//!   engine's storage layer can reach them;
//! - its own `startup.tjs` builds the game UI on a Kirikiri `Window`. The port
//!   implements that class by keeping the window's layer tree
//!   (`Kirikiroid2/src/core/environ/bevy/BevyWindowLayer.cpp`), whose layers
//!   the host draws itself, so once the game drives it the first screen
//!   reaches the host's window. That start-up currently stops earlier, at a
//!   plugin that is not available here, so the game's own title screen does
//!   not appear yet;
//! - it uses plugins that the open-source release does not ship: `PSBFile`
//!   (psbfile.dll, the `.pimg` pictures) and `Motion` (motionplayer.dll). They
//!   are Windows x86 DLLs and the engine cannot load those (upstream compiled
//!   the `LoadLibrary` path out), so nothing stands in for them and start-up
//!   stops with `Member "…" does not exist`. This example reports exactly
//!   where. `Scripts.foreach` / `Scripts.getObjectKeys` (from KAGParserEx.dll)
//!   are reimplemented in `environ/bevy/plugins/ScriptsExt.cpp`, and
//!   `System.addFont` / `CSVParser` are compiled in from this checkout.
//!
//! The point where a game stops is worth reading from the engine's own log,
//! which is where the game's script writes what it could not load;
//! `--dump-log FILE` writes it out after start-up. `atri/data.xp3` gets through
//! `startup.tjs`, `initialize.tjs`, `afterinit.tjs` and `override.tjs` and then
//! stops while building its quick menu, which it defines in PSD files: the log
//! says `psd-file load failed, Member "PSBFile" does not exist`.
//!
//! Input is forwarded to the game's `Window` as the key and mouse events its
//! TJS handlers expect (`krkr_plugin::input`), so its own UI reacts to a click.

use std::fs;
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy_inspector_egui::bevy_egui::EguiPlugin;
use bevy_inspector_egui::quick::WorldInspectorPlugin;
use bevy_krkr::prelude::*;
use bevy_krkr::script::Tjs2Engine;

const DEFAULT_DATA: &str = "test_games/snbk/data.xp3";

fn main() {
    let mut data = None;
    let mut dump_log: Option<String> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            // Writes the engine's own log (the script console, which also
            // carries TJS bytecode disassembly and script warnings) to a file
            // once start-up has stopped. The engine keeps the last few
            // thousand lines in memory, which is where this reads them.
            "--dump-log" => dump_log = args.next(),
            _ => data = Some(arg),
        }
    }
    let data = data.unwrap_or_else(|| DEFAULT_DATA.into());
    info!("snbk: data={data}");

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "bevy_krkr – snbk/data.xp3".into(),
                resolution: (1280, 720).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(EguiPlugin::default())
        .add_plugins(WorldInspectorPlugin::new())
        .add_plugins(BevyKrkrPlugin {
            screen_size: UVec2::new(1280, 720),
        })
        .insert_resource(Session {
            data,
            dump_log,
            startup_failed: false,
        })
        .add_systems(Startup, setup)
        .add_systems(Update, (start_game, log_window, exit_when_done))
        .run();
}

#[derive(Resource)]
struct Session {
    data: String,
    dump_log: Option<String>,
    startup_failed: bool,
}

fn setup(mut commands: Commands, game: Res<Session>, mut enter: MessageWriter<KirikiriEnterEvent>) {
    commands.spawn((Camera2d, KirikiriCamera));
    enter.write(KirikiriEnterEvent {
        data_xp3_path: game.data.clone().into(),
    });
}

/// Adds every archive next to the data file to the engine's storage search
/// path, then runs the game's start-up script.
fn start_game(
    mut game: ResMut<Session>,
    engine: Option<NonSend<Tjs2Engine>>,
    mut done: Local<bool>,
) {
    let Some(engine) = engine else { return };
    if *done {
        return;
    }
    *done = true;

    let dir = Path::new(&game.data).parent().unwrap_or(Path::new("."));
    let mut found: Vec<PathBuf> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "xp3"))
        .collect();
    found.sort();
    let mut mounted = 0;
    for path in &found {
        let Ok(path) = fs::canonicalize(path) else {
            continue;
        };
        // `file:///abs/path.xp3>` is the storage form of "members of this
        // archive": that is what a script's `storage="fgimage720.xp3>x.png"`
        // (or a bare `x.png` found through the search path) resolves against.
        let arg = format!("file://{}>", path.display());
        match engine.exec_script(&format!("Storages.addAutoPath({arg:?});"), "snbk") {
            Ok(()) => {
                mounted += 1;
                debug!("mounted {}", path.display());
            }
            Err(e) => warn!("cannot mount {}: {e}", path.display()),
        }
    }
    info!("snbk: mounted {mounted}/{} archive(s)", found.len());

    // `exec_startup` is the engine's own start-up sequence: `patch.tjs` next
    // to the data file first (games fix up the engine there -- Fal hollow
    // ataraxia defines `GetFileSize`), then `startup.tjs`, then
    // `AfterStartup.tjs`. Running `startup.tjs` alone loses the patch.
    if let Err(e) = engine.exec_startup() {
        // The game could not start at all, so there is nothing left for this
        // session to show; say where it stopped and end it.
        warn!("snbk: the game's startup scripts stopped at: {e}");
        game.startup_failed = true;
    } else {
        info!("snbk: the game's startup scripts completed");
    }

    if let Some(path) = &game.dump_log {
        match engine.eval("Debug.getLastLog(100000)") {
            Ok(log) => match fs::write(path, log.to_string()) {
                Ok(()) => info!("snbk: engine log written to {path}"),
                Err(e) => warn!("snbk: cannot write {path}: {e}"),
            },
            Err(e) => warn!("snbk: cannot read the engine log: {e}"),
        }
    }
}

/// Reports when the game takes the screen over with its own Kirikiri `Window`
/// instead of the host script's screen.
fn log_window(scene: Res<KirikiriScene>, mut shown: Local<Option<UVec2>>) {
    let size = scene.size;
    if size == UVec2::ZERO {
        *shown = None;
        return;
    }
    if *shown != Some(size) {
        info!("snbk: showing a {}x{} screen", size.x, size.y);
        *shown = Some(size);
    }
}

/// Ends the app once the game has ended.
///
/// A game quitting (`System.exit`, `Window.close()`) only closes the Kirikiri
/// *instance* — `KirikiriExitEvent` — and the Bevy app goes on running
/// afterwards, ready for `KirikiriEnterEvent`. A host with its own menu wants
/// exactly that. This example has no screen of its own besides the game's, so
/// it also ends the app; that decision belongs here, not in the plugin.
fn exit_when_done(
    game: Res<Session>,
    mut exited: MessageReader<KirikiriExitEvent>,
    mut app_exit: MessageWriter<AppExit>,
) {
    if exited.read().count() > 0 {
        info!("snbk: the game ended the Kirikiri instance; this example stops too");
        app_exit.write(AppExit::Success);
    } else if game.startup_failed {
        app_exit.write(AppExit::Success);
    }
}
