//! Start a retail game archive through the Bevy host.
//!
//! ```sh
//! cargo run --example snbk -- [data.xp3] [--dump-log PATH]
//! ```
//!
//! Defaults to `test_games/snbk/data.xp3`. Every sibling `.xp3` archive is
//! mounted recursively through Storage, which handles XP3 and the game's 7z archives.
//! The game's startup scripts create its Kirikiri Window and layer tree;
//! Bevy presents the scene and forwards keyboard and mouse input.
//!
//! Native external plugins (`lib*.so`, `.dylib` on macOS) next to the data
//! file or in its `plugin` subdirectory are attached before the start-up
//! scripts run. See `docs/external-plugins.md` and
//! `docs/snbk-acceptance.md` for build commands and observed game behavior.
//! `--dump-log` saves the engine log after startup; the host also logs
//! subsequent timer, animation and input errors.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy_inspector_egui::bevy_egui::EguiPlugin;
use bevy_inspector_egui::quick::WorldInspectorPlugin;
use bevy_krkr::prelude::*;
use bevy_krkr::script::Tjs2Engine;

const DEFAULT_DATA: &str = "test_games/snbk/data.xp3";

#[bevy_main]
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
    #[cfg(target_os = "android")]
    let data = data.unwrap_or_else(|| "/storage/emulated/0/bevy_krkr/game/data.xp3".into());
    #[cfg(not(target_os = "android"))]
    let data = data.unwrap_or_else(|| DEFAULT_DATA.into());
    info!("snbk: data={data}");

    let mut app = App::new();
    #[cfg(feature = "video-ffmpeg")]
    app.add_plugins(KrkrVideoPlugin::default());
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
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
        #[cfg(target_os = "android")]
        storage_requested: false,
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
    #[cfg(target_os = "android")]
    storage_requested: bool,
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
    #[cfg(target_os = "android")]
    {
        let Some(android_app) = bevy::android::ANDROID_APP.get() else {
            return;
        };
        let storage = crate::storage_access::StorageAccess::new(android_app);
        match storage.has_external_storage_access() {
            Ok(true) => {}
            Ok(false) => {
                if !game.storage_requested {
                    game.storage_requested = true;
                    if let Err(error) = storage.request_external_storage_access() {
                        warn!("snbk: cannot request external storage access: {error}");
                        game.startup_failed = true;
                    }
                }
                return;
            }
            Err(error) => {
                warn!("snbk: cannot check external storage access: {error}");
                game.startup_failed = true;
                return;
            }
        }
        let Some(root) = android_app.internal_data_path() else {
            warn!("snbk: Android internal data path is unavailable");
            game.startup_failed = true;
            return;
        };
        bevy_krkr::storage::set_filesystem(std::sync::Arc::new(
            bevy_krkr::storage::AndroidFileSystem::new(root),
        ));
    }
    let Some(engine) = engine else { return };
    if *done {
        return;
    }
    *done = true;

    let data_path = Path::new(&game.data);
    let dir = if data_path.is_dir() {
        data_path
    } else {
        data_path.parent().unwrap_or(Path::new("."))
    };
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
        match engine.exec_script(&format!("Storages.addAutoPathRecursive({arg:?});"), "snbk") {
            Ok(()) => {
                mounted += 1;
                debug!("mounted {}", path.display());
            }
            Err(e) => warn!("cannot mount {}: {e}", path.display()),
        }
    }
    info!("snbk: mounted {mounted}/{} archive(s)", found.len());

    let plugins = load_plugins(&engine, dir);
    info!("snbk: attached {} native plugin(s)", plugins.len());

    let project_dir = match fs::canonicalize(dir) {
        Ok(path) => format!("file://{}/", path.display()),
        Err(error) => {
            warn!(
                "snbk: cannot resolve game directory {}: {error}",
                dir.display()
            );
            game.startup_failed = true;
            return;
        }
    };
    if let Err(error) = engine.exec_script(
        &format!("Storages.setCurrentDirectory({project_dir:?});"),
        "snbk project directory",
    ) {
        warn!("snbk: cannot set game directory: {error}");
        game.startup_failed = true;
        return;
    }

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

#[cfg(target_os = "macos")]
const PLUGIN_EXTENSION: &str = "dylib";
#[cfg(not(target_os = "macos"))]
const PLUGIN_EXTENSION: &str = "so";

/// Attaches every `libNAME.so` next to the data file or in its `plugin`
/// subdirectory, the way the game's scripts would with `Plugins.link`.
///
/// The file attaches as `NAME.dll`, so a later `Plugins.link("NAME.dll")`
/// finds the plugin already loaded. Files without the `lib` prefix are not
/// considered. A plugin that fails to load is reported and skipped.
fn load_plugins(engine: &Tjs2Engine, dir: &Path) -> Vec<String> {
    let mut modules = BTreeMap::new();
    for folder in [dir.to_owned(), dir.join("plugin")] {
        let Ok(entries) = fs::read_dir(folder) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path
                .extension()
                .is_none_or(|extension| extension != PLUGIN_EXTENSION)
            {
                continue;
            }
            let Some(stem) = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .and_then(|stem| stem.strip_prefix("lib"))
            else {
                continue;
            };
            if stem.is_empty() {
                continue;
            }
            modules.entry(format!("{stem}.dll")).or_insert(path);
        }
    }

    let mut attached = Vec::new();
    for (module, path) in modules {
        match engine.exec_script(&format!("Plugins.link({module:?});"), "snbk plugin") {
            Ok(()) => {
                debug!("snbk: attached plugin {module} ({})", path.display());
                attached.push(module);
            }
            Err(error) => warn!("snbk: cannot attach plugin {}: {error}", path.display()),
        }
    }
    attached
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
