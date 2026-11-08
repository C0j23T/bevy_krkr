//! The TJS `Timer` class, and with it every KAG `[wait time=...]`.
//!
//! The timer runs on its own thread and hands its notifications back through
//! `tTVPApplication::PostUserMessage`, so they only reach the script when the
//! host pumps that queue. `krkr_engine_tick` does; without it a timer is
//! created, ticks, and never calls back — which is how a game stalls forever on
//! its first `[wait]`.

use std::ffi::CString;
use std::path::PathBuf;
use std::time::Duration;

use bevy_krkr::script::{last_error, Tjs2Engine};
use bevy_krkr::sys;

fn data_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/game")
}

/// The engine can be initialised once per process, so this is a single test.
#[test]
fn timer_fires_while_the_host_ticks() {
    let path = data_path();
    let c_path = CString::new(path.to_string_lossy().as_bytes()).expect("no NUL in the path");
    // SAFETY: called on the test thread, once for this process.
    let code = unsafe {
        sys::krkr_engine_set_screen_size(640, 480);
        sys::krkr_engine_init(c_path.as_ptr())
    };
    assert_eq!(code, sys::KRKR_OK, "krkr_engine_init: {}", last_error());
    let engine = Tjs2Engine::new().expect("Tjs2Engine::new");

    // `Timer` takes the object whose `onTimer` is to be called; the timer's own
    // `onTimer` event is what fires.
    engine
        .exec_script(
            r#"
                global.fired = 0;
                global.timerOwner = %[];
                global.timer = new Timer(global.timerOwner);
                global.timer.interval = 20;
                global.timer.onTimer = function () { global.fired++; };
                global.timer.enabled = true;
            "#,
            "timer",
        )
        .expect("create a Timer");

    for _ in 0..25 {
        // SAFETY: the engine is running and this is the thread that created it.
        let rc = unsafe { sys::krkr_engine_tick() };
        assert_eq!(rc, sys::KRKR_OK, "tick: {}", last_error());
        std::thread::sleep(Duration::from_millis(20));
    }

    let fired: i32 = engine
        .eval("global.fired")
        .expect("read the counter")
        .to_string()
        .parse()
        .expect("a number");
    assert!(fired > 0, "the timer never called back");
}
