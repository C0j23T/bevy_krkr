//! The Kirikiri side of the host: `host.tjs` and the message window.
//!
//! `host.tjs` runs in Kirikiroid2's TJS2 engine when an instance starts. It
//! builds the screen (a `BitmapLayerTreeOwner`, global `krkrBevy.screen`) and
//! a message-window Layer. [`message_window`] forwards the KAG layer's
//! [`ShowMessageEvent`] / [`ClearMessageEvent`] to it; Kirikiroid2 lays out and
//! rasterises the text, and [`crate::render`] draws the layers.

use std::ffi::CString;

use bevy::prelude::*;
use krkr_kag::{ClearMessageEvent, ShowMessageEvent};
use krkr_script::{last_error, Tjs2Engine};
use krkr_sys as sys;

/// TJS2 source of the host (`krkr_plugin/src/host.tjs`).
pub const HOST_SCRIPT: &str = include_str!("host.tjs");

/// TJS2 expression of the screen object read by [`crate::render`].
pub const SCREEN_EXPR: &str = "krkrBevy.screen";

/// Handle to the screen layer tree. Non-send resource; must be dropped before
/// the engine shuts down (see [`crate::teardown`]).
pub struct KrkrScreen {
    ptr: *mut sys::KrkrLayerOpaque,
}

impl KrkrScreen {
    pub fn as_ptr(&self) -> *mut sys::KrkrLayerOpaque {
        self.ptr
    }
}

impl Drop for KrkrScreen {
    fn drop(&mut self) {
        // SAFETY: ptr came from krkr_layer_find and is released once.
        unsafe { sys::krkr_layer_release(self.ptr) };
    }
}

/// Run `host.tjs`, create the host object for a `size` screen and return a
/// handle to its screen.
pub fn start(engine: &Tjs2Engine, size: UVec2) -> Result<KrkrScreen, String> {
    engine
        .exec_script(HOST_SCRIPT, "host.tjs")
        .map_err(|e| e.to_string())?;
    // Numbers are formatted by Rust; nothing user-provided is spliced in.
    let init = format!("global.krkrBevy = new KrkrBevyHost({}, {});", size.x, size.y);
    engine
        .exec_script(&init, "bevy_krkr")
        .map_err(|e| e.to_string())?;
    let expr = CString::new(SCREEN_EXPR).expect("no NUL");
    // SAFETY: the engine is running (engine exists); expr is NUL-terminated.
    let ptr = unsafe { sys::krkr_layer_find(expr.as_ptr()) };
    if ptr.is_null() {
        return Err(format!("{SCREEN_EXPR}: {}", last_error()));
    }
    Ok(KrkrScreen { ptr })
}

/// Release the host objects (`host.tjs`). Call before the engine shuts down.
///
/// TJS2 collects objects by reference count and has no cycle collector, so the
/// screen's layer tree — Layer ↔ layer manager ↔ tree owner — never reaches a
/// reference count of zero on its own. Its bitmaps stay allocated for the rest
/// of the process unless the script tears it down, which is several megabytes
/// per stopped instance.
pub fn stop(engine: &Tjs2Engine) {
    const TEARDOWN: &str =
        "if (global.krkrBevy !== void) { global.krkrBevy.finalize(); global.krkrBevy = void; }";
    if let Err(e) = engine.exec_script(TEARDOWN, "bevy_krkr teardown") {
        warn!("bevy_krkr: releasing the host objects failed: {e}");
    }
}

/// Draws the KAG message text into the Kirikiri message window.
pub fn message_window(
    engine: Option<NonSend<Tjs2Engine>>,
    mut show: MessageReader<ShowMessageEvent>,
    mut clear: MessageReader<ClearMessageEvent>,
) {
    let Some(engine) = engine else {
        show.clear();
        clear.clear();
        return;
    };
    // A game that quit with `System.exit` took the interpreter with it.
    if !engine.is_running() {
        show.clear();
        clear.clear();
        return;
    }
    // A clear is always followed by new text (KagCursor clears before it
    // shows), so apply clears first, then the latest text.
    if clear.read().count() > 0 {
        if let Err(e) = engine.call("krkrBevy.clearMessage", &[]) {
            warn!("bevy_krkr: clearMessage: {e}");
        }
    }
    if let Some(m) = show.read().last() {
        if let Err(e) = engine.call("krkrBevy.showMessage", &[&m.text]) {
            warn!("bevy_krkr: showMessage: {e}");
        }
    }
}
