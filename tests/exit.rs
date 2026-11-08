//! How a game ends itself, and what is left of the session afterwards.
//!
//! A game quits in one of three ways: `System.exit` (shuts the engine down and
//! then leaves, never returning), `System.terminate`, and `Window.close()` on
//! what is the main window -- a native window manager answers the last one by
//! destroying the window, which then ends the application. The host has to
//! survive all three: it keeps running the process and closes the session, and
//! the objects it holds into the interpreter are gone by then.
//!
//! Single test, because the sessions share the process.

use std::ffi::CString;
use std::path::PathBuf;

use bevy::prelude::UVec2;
use bevy_krkr::plugin::host;
use bevy_krkr::script::{last_error, Tjs2Engine};
use bevy_krkr::sys;

fn data_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/game")
}

fn start(name: &str) -> (Tjs2Engine, host::KrkrScreen) {
    let path = data_path();
    let c_path = CString::new(path.to_string_lossy().as_bytes()).expect("no NUL in the path");
    // SAFETY: called on the test thread, one session at a time; c_path is
    // NUL-terminated.
    let code = unsafe {
        sys::krkr_engine_set_screen_size(800, 600);
        sys::krkr_engine_init(c_path.as_ptr())
    };
    assert_eq!(code, sys::KRKR_OK, "{name}: krkr_engine_init: {}", last_error());
    let engine = Tjs2Engine::new().unwrap_or_else(|e| panic!("{name}: {e}"));
    // `System.exit` and `System.terminate` do nothing until the start-up script
    // has run (upstream guards them with `TVPStartupSuccess`): a game cannot
    // quit before it is up. Every KAG game ships a `startup.tjs`, and so does
    // the fixture; running it is what a host does before handing over.
    engine
        .exec_startup()
        .unwrap_or_else(|e| panic!("{name}: start-up: {e}"));
    assert_eq!(
        engine.eval("global.fixtureStartup").unwrap(),
        "ran",
        "{name}: startup.tjs did not run"
    );
    let screen =
        host::start(&engine, UVec2::new(800, 600)).unwrap_or_else(|e| panic!("{name}: {e}"));
    (engine, screen)
}

/// Stops the session the way [`bevy_krkr::plugin::teardown`] does.
fn stop(engine: Tjs2Engine, screen: host::KrkrScreen) {
    if engine.is_running() {
        host::stop(&engine);
    }
    drop(screen);
    drop(engine);
    // SAFETY: every handle created for the session is released.
    unsafe { sys::krkr_engine_shutdown() };
}

#[test]
fn a_game_can_end_the_session_and_a_new_one_can_start() {
    // `System.exit` is the engine's own `exit()`: it tears the engine down and
    // never returns. The host must not unwind through the frames that are left,
    // and must not take its objects apart a second time either.
    let (engine, screen) = start("exit");
    assert!(engine.is_running(), "exit: the engine is up");
    let error = engine
        .exec_script("System.exit(3);", "quit")
        .expect_err("exit: System.exit returned");
    assert!(
        error.to_string().contains("requested exit (code 3)"),
        "exit: unexpected message: {error}"
    );
    // SAFETY: no preconditions.
    let (requested, running) = unsafe {
        (
            sys::krkr_engine_exit_requested(),
            sys::krkr_engine_is_running(),
        )
    };
    assert_ne!(requested, 0, "exit: the host was not told about the exit");
    assert_eq!(running, 0, "exit: the engine is still running");
    stop(engine, screen);

    // The next session starts cleanly, and the engine is not stuck reporting
    // the previous session's exit.
    let (engine, screen) = start("after exit");
    assert_eq!(
        engine.eval("1 + 1").unwrap(),
        "2",
        "after exit: evaluation"
    );
    // SAFETY: no preconditions.
    assert_eq!(
        unsafe { sys::krkr_engine_exit_requested() },
        0,
        "after exit: exit request left over"
    );

    // `Window.close()`: a native window manager destroys the window, and the
    // main window going away ends the application. The port hides it at once
    // and destroys it on the next tick, so a script that closed its own window
    // keeps running instead of being taken apart under its own call.
    let (mut width, mut height) = (0u32, 0u32);
    engine
        .exec_script(
            r#"
                global.w = new Window();
                w.setInnerSize(64, 48);
                global.wl = new Layer(w, null);
                wl.setImageSize(64, 48);
                wl.setSizeToImageSize();
                w.visible = true;
                w.update();
            "#,
            "window test",
        )
        .unwrap_or_else(|e| panic!("after exit: {e}"));
    // SAFETY: an instance is running; out-parameters are valid.
    let shown = unsafe { sys::krkr_window_snapshot(&mut width, &mut height, std::ptr::null_mut()) };
    assert!(!shown.is_null(), "after exit: the window is not shown");

    engine
        .exec_script(
            r#"
                global.survived = "no";
                w.close();
                global.survived = "yes"; // `close()` does not end the caller
            "#,
            "window test",
        )
        .unwrap_or_else(|e| panic!("after exit: {e}"));
    assert_eq!(
        engine.eval("global.survived").unwrap(),
        "yes",
        "after exit: close() ended the calling script"
    );
    // Nothing for the host to show any more, and the window still exists.
    // SAFETY: an instance is running; out-parameters are valid.
    let hidden = unsafe { sys::krkr_window_snapshot(&mut width, &mut height, std::ptr::null_mut()) };
    assert!(hidden.is_null(), "after exit: a closed window is shown");
    assert_eq!(
        unsafe { sys::krkr_engine_exit_requested() },
        0,
        "after exit: the window was destroyed during close()"
    );

    // The tick destroys it, and it was the main window.
    // SAFETY: an instance is running.
    unsafe { sys::krkr_engine_tick() };
    assert_ne!(
        unsafe { sys::krkr_engine_exit_requested() },
        0,
        "after exit: closing the main window did not end the application"
    );

    stop(engine, screen);
}
