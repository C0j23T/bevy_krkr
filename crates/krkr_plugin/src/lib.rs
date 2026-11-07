//! `krkr_plugin` – assembles bevy_krkr and owns the Kirikiri lifecycle.
//!
//! - Write [`KirikiriEnterEvent`] to start an instance. The plugin calls
//!   `krkr_engine_init` on the data path, gets the TJS2 engine, runs the host
//!   script ([`host`]), creates the KAG parser (all as non-send resources) and
//!   loads the start scenario.
//! - [`KirikiriExitEvent`] is written by the plugin when script code asks the
//!   engine to terminate, or by game code. On it, the instance is torn down in
//!   dependency order: parser, screen, TJS2 engine, archive, engine shutdown.
//!   Kirikiroid2 supports one instance per process: after an exit, a new
//!   [`KirikiriEnterEvent`] fails with an error.
//!
//! Kirikiri output is drawn by the camera marked with [`KirikiriCamera`]; see
//! [`render`]. Engine log lines go to Bevy's log (`bevy_krkr::engine`).

pub mod host;
pub mod render;

use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::path::{Path, PathBuf};

use bevy::app::AppExit;
use bevy::ecs::system::SystemState;
use bevy::prelude::*;
use krkr_kag::{KagCursor, KagParser, KagScriptLoadedEvent};
use krkr_script::{last_error, Tjs2Engine};
use krkr_sys as sys;

pub use krkr_audio::{BgmChannel, KrkrAudioPlugin, KrkrAudioSettings, SeChannel, VoiceChannel};
pub use krkr_kag::{KagPlugin, KirikiriCamera};
pub use krkr_script::Tjs2Plugin;
pub use host::KrkrScreen;
pub use render::KrkrRenderPlugin;

/// Start a Kirikiri instance.
#[derive(Message, Debug, Clone)]
pub struct KirikiriEnterEvent {
    /// A `data.xp3` archive or an unpacked data folder.
    pub data_xp3_path: PathBuf,
}

/// Stop the running Kirikiri instance (or: it has asked to stop).
#[derive(Message, Debug, Clone, Default)]
pub struct KirikiriExitEvent;

/// Whether an instance is running.
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub enum KirikiriState {
    #[default]
    Idle,
    Running {
        data_path: PathBuf,
    },
}

/// Plugin configuration, available as a resource.
#[derive(Resource, Debug, Clone)]
pub struct KirikiriConfig {
    /// Scenario loaded after start-up.
    pub start_storage: String,
    /// Size of the Kirikiri screen in pixels.
    pub screen_size: UVec2,
}

/// Engine-level state of a running instance. Non-send resource; dropping it
/// closes the archive and shuts the engine down, so it must outlive the
/// [`Tjs2Engine`] and [`KagParser`] (see [`teardown`]).
pub struct KrkrSession {
    archive: *mut sys::XP3ArchiveOpaque,
    data_path: PathBuf,
}

impl KrkrSession {
    pub fn data_path(&self) -> &Path {
        &self.data_path
    }

    /// Read a member of the mounted `data.xp3`. `None` for data folders or
    /// missing members.
    pub fn read_file(&self, name: &str) -> Option<Vec<u8>> {
        if self.archive.is_null() {
            return None;
        }
        let name = CString::new(name).ok()?;
        let mut data: *mut u8 = std::ptr::null_mut();
        let mut len: usize = 0;
        // SAFETY: archive is live; out-pointers are valid.
        let code =
            unsafe { sys::krkr_xp3_read_file(self.archive, name.as_ptr(), &mut data, &mut len) };
        if code != sys::KRKR_OK || data.is_null() {
            return None;
        }
        // SAFETY: the C ABI returns `len` readable bytes at `data`, freed
        // once below with krkr_free.
        let bytes = unsafe { std::slice::from_raw_parts(data, len) }.to_vec();
        unsafe { sys::krkr_free(data.cast()) };
        Some(bytes)
    }
}

impl Drop for KrkrSession {
    fn drop(&mut self) {
        // SAFETY: archive came from krkr_xp3_open; engine was initialised when
        // the session was created. Each is released once.
        unsafe {
            if !self.archive.is_null() {
                sys::krkr_xp3_close(self.archive);
            }
            sys::krkr_engine_shutdown();
        }
    }
}

/// Adds every bevy_krkr subsystem. Requires `DefaultPlugins` (or equivalent:
/// assets, time, input, rendering) to be added first.
pub struct BevyKrkrPlugin {
    /// Scenario loaded when an instance starts. Default `"first.ks"`.
    pub start_storage: String,
    /// Size of the Kirikiri screen. Default 800x600.
    pub screen_size: UVec2,
}

impl Default for BevyKrkrPlugin {
    fn default() -> Self {
        Self {
            start_storage: "first.ks".into(),
            screen_size: UVec2::new(800, 600),
        }
    }
}

impl Plugin for BevyKrkrPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<KirikiriEnterEvent>()
            .add_message::<KirikiriExitEvent>()
            .init_resource::<KirikiriState>()
            .insert_resource(KirikiriConfig {
                start_storage: self.start_storage.clone(),
                screen_size: self.screen_size,
            })
            .add_plugins((Tjs2Plugin, KagPlugin, KrkrAudioPlugin, KrkrRenderPlugin))
            .add_systems(PreUpdate, handle_enter)
            .add_systems(PostUpdate, poll_engine_exit)
            .add_systems(Last, handle_exit);
    }
}

fn handle_enter(world: &mut World, reader: &mut SystemState<MessageReader<KirikiriEnterEvent>>) {
    let requests: Vec<PathBuf> = match reader.get_mut(world) {
        Ok(mut r) => r.read().map(|e| e.data_xp3_path.clone()).collect(),
        Err(_) => return,
    };
    for path in requests {
        if world.contains_non_send::<KrkrSession>() {
            warn!(
                "bevy_krkr: KirikiriEnterEvent({}) ignored, an instance is already running",
                path.display()
            );
            continue;
        }
        let config = world.resource::<KirikiriConfig>().clone();
        let start_storage = config.start_storage.clone();
        match start(world, &path, &config) {
            Ok(()) => info!("bevy_krkr: started {} ({start_storage})", path.display()),
            Err(e) => {
                error!("bevy_krkr: failed to start {}: {e}", path.display());
                teardown(world);
                world.write_message(KirikiriExitEvent);
            }
        }
    }
}

/// Engine log lines → Bevy log. Called synchronously from engine calls.
unsafe extern "C" fn engine_log(message: *const c_char, important: c_int, _: *mut c_void) {
    if message.is_null() {
        return;
    }
    // SAFETY: the C ABI passes a NUL-terminated UTF-8 string valid for the call.
    let message = unsafe { CStr::from_ptr(message) }.to_string_lossy();
    if important != 0 {
        info!(target: "bevy_krkr::engine", "{message}");
    } else {
        debug!(target: "bevy_krkr::engine", "{message}");
    }
}

fn start(world: &mut World, path: &Path, config: &KirikiriConfig) -> Result<(), String> {
    let c_path = CString::new(path.to_string_lossy().as_bytes()).map_err(|e| e.to_string())?;
    let size = config.screen_size;
    // SAFETY: called on the main thread (exclusive system); the callback is a
    // 'static fn needing no userdata; c_path is NUL-terminated.
    let code = unsafe {
        sys::krkr_engine_set_log_callback(Some(engine_log), std::ptr::null_mut());
        sys::krkr_engine_set_screen_size(size.x as c_int, size.y as c_int);
        sys::krkr_engine_init(c_path.as_ptr())
    };
    if code != sys::KRKR_OK {
        return Err(format!("krkr_engine_init: {}", last_error()));
    }
    // The engine is up: from here on, a session owns its shutdown.
    let archive = if path.is_file() {
        // SAFETY: the engine is running; c_path is NUL-terminated.
        let archive = unsafe { sys::krkr_xp3_open(c_path.as_ptr()) };
        if archive.is_null() {
            let err = format!("krkr_xp3_open({}): {}", path.display(), last_error());
            world.insert_non_send(KrkrSession {
                archive,
                data_path: path.to_owned(),
            });
            return Err(err);
        }
        archive
    } else {
        std::ptr::null_mut()
    };
    world.insert_non_send(KrkrSession {
        archive,
        data_path: path.to_owned(),
    });

    let engine = Tjs2Engine::new().map_err(|e| e.to_string())?;
    let screen = host::start(&engine, size)?;
    let mut parser = KagParser::new(&engine).map_err(|e| e.to_string())?;
    parser.load(&config.start_storage).map_err(|e| e.to_string())?;
    world.insert_non_send(engine);
    world.insert_non_send(screen);
    world.insert_non_send(parser);

    world.resource_mut::<KagCursor>().start(&config.start_storage);
    world.write_message(KagScriptLoadedEvent {
        storage: config.start_storage.clone(),
    });
    *world.resource_mut::<KirikiriState>() = KirikiriState::Running {
        data_path: path.to_owned(),
    };
    Ok(())
}

/// Drop the running instance in dependency order. Does nothing when idle.
pub fn teardown(world: &mut World) {
    drop(world.remove_non_send::<KagParser>());
    drop(world.remove_non_send::<KrkrScreen>());
    drop(world.remove_non_send::<Tjs2Engine>());
    let was_running = world.remove_non_send::<KrkrSession>().is_some();
    if let Some(mut cursor) = world.get_resource_mut::<KagCursor>() {
        cursor.reset();
    }
    if let Some(mut state) = world.get_resource_mut::<KirikiriState>() {
        *state = KirikiriState::Idle;
    }
    if was_running {
        info!("bevy_krkr: instance stopped");
    }
}

fn poll_engine_exit(
    session: Option<NonSend<KrkrSession>>,
    mut exit: MessageWriter<KirikiriExitEvent>,
) {
    if session.is_none() {
        return;
    }
    // SAFETY: an instance is running; main thread via NonSend.
    if unsafe { sys::krkr_engine_exit_requested() } != 0 {
        exit.write(KirikiriExitEvent);
    }
}

fn handle_exit(
    world: &mut World,
    readers: &mut SystemState<(MessageReader<KirikiriExitEvent>, MessageReader<AppExit>)>,
) {
    let requested = match readers.get_mut(world) {
        Ok((mut exit, mut app_exit)) => exit.read().count() + app_exit.read().count() > 0,
        Err(_) => false,
    };
    if requested {
        teardown(world);
    }
}
