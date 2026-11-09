//! An instance can be stopped and another one started in the same process.
//!
//! Kirikiroid2 initialises a lot of process-global state (the TJS2 interpreter,
//! the event system, the storage caches) and upstream assumes the process ends
//! right after a shutdown. `krkr_engine_shutdown` therefore has to clear what
//! survives, and `krkr_engine_init` has to reset the one-shot guards; this test
//! runs three sessions in a row to prove it.
//!
//! Single test, because the sessions share the process: cargo runs each test in
//! its own binary/thread set, and two of them would fight over the globals.

use std::ffi::CString;
use std::path::PathBuf;

use bevy::prelude::UVec2;
use bevy_krkr::plugin::host;
use bevy_krkr::script::{last_error, Tjs2Engine};
use bevy_krkr::sys;

/// `assets/game` of the workspace.
fn data_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/game")
}

/// Start an instance, use it (script, host layer) and stop it.
fn run_session(name: &str) {
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
    let screen = host::start(&engine, UVec2::new(800, 600)).unwrap_or_else(|e| panic!("{name}: {e}"));

    // The interpreter runs, and the session starts without a pending exit
    // request from the previous one.
    assert_eq!(engine.eval("1 + 1").unwrap(), "2", "{name}: evaluation");
    assert_eq!(
        engine.eval("typeof krkrBevy").unwrap(),
        "Object",
        "{name}: host.tjs ran"
    );
    // A plugin is attached when a script links it, as a game does (`System`
    // .addFont from plugins/addFont.cpp). Which modules are already attached
    // survives a shutdown, so this also proves the next session attaches them
    // again instead of skipping them.
    engine
        .exec_script(r#"Plugins.link("addFont.dll");"#, "link")
        .unwrap_or_else(|e| panic!("{name}: {e}"));
    assert_eq!(
        engine.eval("typeof System.addFont").unwrap(),
        "Object",
        "{name}: System.addFont is missing"
    );
    assert_eq!(
        unsafe { sys::krkr_engine_exit_requested() },
        0,
        "{name}: exit request left over from a previous session"
    );

    // Engine error messages come from a table the engine frees when a session
    // ends; it has to be usable again in the next one rather than dangle.
    let error = engine
        .eval("this is not valid TJS")
        .expect_err("{name}: invalid script was accepted");
    let message = error.to_string();
    assert!(
        message.contains("Syntax error"),
        "{name}: TJS error message looks broken: {message:?}"
    );

    // Release the handles in the plugin's order.
    drop(screen);
    drop(engine);
    // SAFETY: every handle created above is released.
    unsafe { sys::krkr_engine_shutdown() };
}

#[test]
fn restart_in_one_process() {
    run_session("first");
    run_session("second");
    run_session("third");
}
