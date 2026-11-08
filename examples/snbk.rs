//! Start a retail game archive (`snbk/data.xp3`, KAG3) through the bevy_krkr
//! host.
//!
//! ```sh
//! cargo run --example snbk -- [data.xp3] [scenario storage] [--game-startup] [--dump-log PATH]
//! ```
//!
//! Defaults: `snbk/data.xp3` and `scenario\start.ks` (the archive's own entry
//! point). The scenario is parsed by Kirikiroid2's KAG parser out of the
//! mounted archive, and the resulting tags become Bevy messages through
//! `krkr_kag`; the message window is drawn by the host script and reaches the
//! window through the Kirikiri screen texture.
//!
//! What this archive needs on top of the plain engine:
//! - the game keeps its voice, CG and event images in **7z** archives next to
//!   `data.xp3` (`voice.xp3`, `fgimage720.xp3`, …), not in XP3. They are opened
//!   through libarchive (`Kirikiroid2/src/core/environ/bevy/BevyArchives.cpp`),
//!   and this example mounts every archive next to the data file so the
//!   engine's storage layer can reach them;
//! - its own `startup.tjs` builds the game UI on a Kirikiri `Window`. The port
//!   implements that class by rendering it off-screen
//!   (`Kirikiroid2/src/core/environ/bevy/BevyWindowLayer.cpp`), so once the
//!   game drives it the first screen reaches the host's window. That start-up
//!   currently stops earlier, at a plugin that is not available here, so the
//!   game's own title screen does not appear yet; the scenario path below
//!   does, and this example says which of the two is on screen;
//! - it uses plugins that the open-source release does not ship: `PSBFile`
//!   (psbfile.dll, the `.pimg` pictures) and `Motion` (motionplayer.dll). They
//!   are Windows x86 DLLs and the engine cannot load those (upstream compiled
//!   the `LoadLibrary` path out), so nothing stands in for them and start-up
//!   stops with `Member "…" does not exist`. `--game-startup` reports exactly
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
//! Input goes two ways: the host's own scenario advances on a click (or Enter /
//! Space), and everything is also forwarded to the game's `Window` as the key
//! and mouse events its TJS handlers expect (`krkr_plugin::input`), so a game
//! that brought its own UI reacts to the same click.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use bevy::prelude::*;
use bevy_krkr::prelude::*;
use bevy_krkr::script::Tjs2Engine;

const DEFAULT_DATA: &str = "test_games/snbk/data.xp3";
const DEFAULT_STORAGE: &str = "scenario\\start.ks";
/// How long to stay up when the scenario produces nothing (it is waiting for a
/// click, or stopped at a tag this host does not interpret).
const IDLE_TIMEOUT: Duration = Duration::from_secs(120);

fn main() {
    let mut positional = Vec::new();
    let mut game_startup = false;
    let mut dump_log: Option<String> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--game-startup" => game_startup = true,
            // Writes the engine's own log (the script console, which also
            // carries TJS bytecode disassembly and script warnings) to a file
            // once start-up has stopped. The engine keeps the last few
            // thousand lines in memory, which is where this reads them.
            "--dump-log" => dump_log = args.next(),
            _ => positional.push(arg),
        }
    }
    let data = positional.first().cloned().unwrap_or_else(|| DEFAULT_DATA.into());
    let storage = positional
        .get(1)
        .cloned()
        .unwrap_or_else(|| DEFAULT_STORAGE.into());
    info!("snbk: data={data} storage={storage} game_startup={game_startup}");

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "bevy_krkr – snbk/data.xp3".into(),
                resolution: (1280, 720).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(BevyKrkrPlugin {
            start_storage: storage,
            screen_size: UVec2::new(1280, 720),
        })
        .insert_resource(KirikiriSession {
            data,
            game_startup,
            dump_log,
            startup_failed: false,
        })
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (mount_archives, log_kag, log_window, exit_when_done),
        )
        .run();
}

#[derive(Resource)]
struct KirikiriSession {
    data: String,
    game_startup: bool,
    dump_log: Option<String>,
    startup_failed: bool,
}

fn setup(mut commands: Commands, game: Res<KirikiriSession>, mut enter: MessageWriter<KirikiriEnterEvent>) {
    commands.spawn((Camera2d, KirikiriCamera));
    enter.write(KirikiriEnterEvent {
        data_xp3_path: game.data.clone().into(),
    });
}

/// Adds every archive next to the data file to the engine's storage search
/// path and, with `--game-startup`, runs the game's start-up script.
fn mount_archives(
    mut game: ResMut<KirikiriSession>,
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

    let mut startup_failed = false;
    if game.game_startup {
        // `exec_startup` is the engine's own start-up sequence: `patch.tjs` next
        // to the data file first (games fix up the engine there -- Fal hollow
        // ataraxia defines `GetFileSize`), then `startup.tjs`, then
        // `AfterStartup.tjs`. Running `startup.tjs` alone loses the patch.
        match engine.exec_startup() {
            Ok(()) => info!("snbk: the game's startup scripts completed"),
            Err(e) => {
                // The game could not start at all, so there is nothing left for
                // this session to show; say where it stopped and end it.
                warn!("snbk: the game's startup scripts stopped at: {e}");
                startup_failed = true;
            }
        }
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

    if startup_failed {
        game.startup_failed = true;
    }
}

fn log_kag(
    mut text: MessageReader<ShowMessageEvent>,
    mut bgm: MessageReader<PlayBgmEvent>,
    mut images: MessageReader<ShowSpriteEvent>,
    mut tags: MessageReader<UnhandledTagEvent>,
) {
    for m in text.read() {
        info!("text: {:?}", m.text);
    }
    for b in bgm.read() {
        info!("bgm: {}", b.storage);
    }
    for i in images.read() {
        info!("image: layer {} storage {:?}", i.layer, i.storage);
    }
    // Every tag this host has no handler for: games define their own macros on
    // top of KAG, so this is the list to work through for a given title.
    for tag in tags.read() {
        debug!(target: "snbk::tag", "unhandled: {} {:?}", tag.name, tag.attrs);
    }
}

/// Reports when the game takes the screen over with its own Kirikiri `Window`
/// (KAG's `KAGWindow`) instead of the host script's screen.
fn log_window(engine: Option<NonSend<Tjs2Engine>>, mut shown: Local<Option<UVec2>>) {
    if engine.is_none() {
        return;
    }
    let (mut width, mut height) = (0u32, 0u32);
    // SAFETY: an instance is running (the engine resource exists); the NULL
    // revision out-parameter is allowed.
    let ptr = unsafe {
        bevy_krkr::sys::krkr_window_snapshot(&mut width, &mut height, std::ptr::null_mut())
    };
    if ptr.is_null() {
        return;
    }
    let size = UVec2::new(width, height);
    if *shown != Some(size) {
        info!("snbk: the game's own window is showing ({width}x{height})");
        *shown = Some(size);
    }
}

/// Ends the app once the game has ended.
///
/// The two endings are not the same thing and the plugin keeps them apart: a
/// game quitting (`System.exit`, `Window.close()`) only closes the Kirikiri
/// *instance* — `KirikiriExitEvent`, KAG's system variables saved — and the
/// Bevy app goes on running afterwards, ready for `KirikiriEnterEvent`. A host
/// with its own menu wants exactly that. This example has no screen of its own
/// besides the game's, so it also ends the app; that decision belongs here, not
/// in the plugin.
fn exit_when_done(
    time: Res<Time>,
    game: Res<KirikiriSession>,
    mut since: Local<Option<Duration>>,
    mut loaded: MessageReader<KagScriptLoadedEvent>,
    mut ended: MessageReader<KagScriptEndEvent>,
    mut exited: MessageReader<KirikiriExitEvent>,
    mut app_exit: MessageWriter<AppExit>,
) {
    for e in loaded.read() {
        info!("scenario loaded: {}", e.storage);
        *since = Some(time.elapsed());
    }
    // With `--game-startup` the game runs its own KAG loop and the storage given
    // on the command line is only what this host parses alongside it, so its end
    // is not the end of the session. A game whose start-up threw is a different
    // matter: it will never get anywhere.
    let scenario_ended = !game.game_startup && ended.read().count() > 0;
    let instance_ended = exited.read().count() > 0;
    if scenario_ended || game.startup_failed || instance_ended {
        if instance_ended {
            info!("snbk: the game ended the Kirikiri instance; this example stops too");
        }
        app_exit.write(AppExit::Success);
    } else if since.is_some_and(|start| time.elapsed() > start + IDLE_TIMEOUT) {
        warn!("snbk: idle for {}s, exiting", IDLE_TIMEOUT.as_secs());
        app_exit.write(AppExit::Success);
    }
}
