//! Bevy input reaches the game.
//!
//! KAG's own UI reacts to the TJS `Window` handlers `onKeyDown` / `onMouseDown`
//! / `onMouseMove`, and only the host can post those. `krkr_plugin::input`
//! turns Bevy's input state into them; this drives that system with a real
//! `ButtonInput` and checks the handler in TJS saw the key.
//!
//! Keys only: a mouse position cannot be faked through Bevy's `Window` (winit
//! owns it), while the coordinate mapping is covered by the unit tests in the
//! module itself.

use std::ffi::CString;
use std::path::PathBuf;

use bevy::input::InputPlugin;
use bevy::prelude::*;
use bevy_krkr::plugin::input;
use bevy_krkr::prelude::*;
use bevy_krkr::script::{last_error, Tjs2Engine};
use bevy_krkr::sys;

fn data_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/game")
}

/// Presses a key, the way Bevy's own input systems would.
fn press_enter(mut keys: ResMut<ButtonInput<KeyCode>>) {
    keys.press(KeyCode::Enter);
}

#[test]
fn a_key_press_reaches_the_game() {
    let path = data_path();
    let c_path = CString::new(path.to_string_lossy().as_bytes()).expect("no NUL in the path");
    // SAFETY: called on the test thread, once for this process.
    let code = unsafe {
        sys::krkr_engine_set_screen_size(320, 240);
        sys::krkr_engine_init(c_path.as_ptr())
    };
    assert_eq!(code, sys::KRKR_OK, "krkr_engine_init: {}", last_error());
    let engine = Tjs2Engine::new().expect("Tjs2Engine::new");

    // A window with a key handler, shown and drawn: the engine only accepts
    // host input while a window is up.
    engine
        .exec_script(
            r#"
                global.w = new Window();
                w.setInnerSize(320, 240);
                global.wl = new Layer(w, null);
                wl.setImageSize(320, 240);
                wl.setSizeToImageSize();
                wl.fillRect(0, 0, 320, 240, 0xff000000);
                global.keys = "";
                w.onKeyDown = function(key, shift) { global.keys += "down" + key + "@" + shift + " "; };
                w.visible = true;
                w.update();
            "#,
            "input test",
        )
        .expect("window with a key handler");

    let mut app = App::new();
    app.add_plugins((MinimalPlugins, InputPlugin))
        .insert_resource(KirikiriState::Running {
            data_path: data_path(),
        })
        // `press_enter` stands in for the keyboard; the forwarding system has to
        // see the key in the same frame, after Bevy has collected input.
        .add_systems(
            PreUpdate,
            (press_enter, input::forward_keys)
                .chain()
                .after(bevy::input::InputSystems),
        );
    app.update();

    // The engine delivers queued input events on its next tick.
    // SAFETY: an instance is running.
    unsafe { sys::krkr_engine_tick() };

    assert_eq!(
        engine.eval("global.keys").unwrap(),
        // VK_RETURN is 13; no modifier was held.
        "down13@0 ",
        "the key did not reach the window's onKeyDown"
    );

    drop(engine);
    // SAFETY: every handle created above is released.
    unsafe { sys::krkr_engine_shutdown() };
}
