//! Stopping an instance must give its memory back.
//!
//! Upstream Kirikiroid2 assumes the process ends after `TVPSystemUninit`, so a
//! host that runs one session after another would leak everything a session
//! allocated. This guards the traps found there:
//!
//! - the screen's layer tree, held by a reference cycle (Layer ↔ layer manager
//!   ↔ tree owner): several megabytes per session;
//! - TJS2 script blocks and their code contexts, which reference each other and
//!   are only reachable from the argument stack's leftover variants;
//! - the process-global state that upstream only frees at exit (event system,
//!   storage caches, font rasterisers, sound buffers).
//!
//! Single test: the sessions share the process, and cargo would run two tests
//! in parallel threads over the same globals.

mod common;

use std::ffi::CString;
use std::path::PathBuf;

use bevy::prelude::UVec2;
use bevy_krkr::kag::KagParser;
use bevy_krkr::plugin::host;
use bevy_krkr::script::Tjs2Engine;
use bevy_krkr::sys;

/// glibc's `mallinfo2`, used for "bytes handed out by malloc".
#[repr(C)]
struct MallInfo2 {
    arena: usize,
    ordblks: usize,
    smblks: usize,
    hblks: usize,
    hblkhd: usize,
    usmblks: usize,
    fsmblks: usize,
    uordblks: usize,
    fordblks: usize,
    keepcost: usize,
}

extern "C" {
    fn mallinfo2() -> MallInfo2;
}

fn bytes_in_use() -> usize {
    // SAFETY: mallinfo2 takes no arguments.
    unsafe { mallinfo2().uordblks }
}

/// Bytes one stopped session may keep. Sessions are reclaimed completely; what
/// is left is allocator noise (measured at about 3 KiB).
const BUDGET_PER_SESSION: usize = 32 * 1024;

/// Start an instance, use it the way the plugin does, stop it.
fn session(name: &str) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/game");
    let c_path = CString::new(path.to_string_lossy().as_bytes()).expect("no NUL in the path");
    // SAFETY: one session at a time on this thread; c_path is NUL-terminated.
    let code = unsafe {
        sys::krkr_engine_set_screen_size(800, 600);
        sys::krkr_engine_init(c_path.as_ptr())
    };
    assert_eq!(code, sys::KRKR_OK, "{name}: krkr_engine_init");

    let engine = Tjs2Engine::new().unwrap_or_else(|e| panic!("{name}: {e}"));
    let screen =
        host::start(&engine, UVec2::new(800, 600)).unwrap_or_else(|e| panic!("{name}: {e}"));
    let mut parser = KagParser::new(&engine).unwrap_or_else(|e| panic!("{name}: {e}"));
    parser
        .load("first.ks")
        .unwrap_or_else(|e| panic!("{name}: {e}"));
    engine.eval("1 + 1").expect("evaluation");

    // The plugin's order: parser, host objects, screen, engine handle, engine.
    drop(parser);
    drop(screen);
    host::stop(&engine);
    drop(engine);
    // SAFETY: every handle created above is released.
    unsafe { sys::krkr_engine_shutdown() };
}

#[test]
fn stopping_a_session_frees_its_memory() {
    common::initialize_renderer();
    for i in 0..8 {
        session(&format!("warm-up {i}"));
        assert_eq!(bevy_krkr::plugin::render::texture_usage(), (0, 0));
    }

    let before = bytes_in_use();
    const SESSIONS: usize = 3;
    for i in 0..SESSIONS {
        session(&format!("measured {i}"));
        assert_eq!(bevy_krkr::plugin::render::texture_usage(), (0, 0));
    }
    let growth = bytes_in_use().saturating_sub(before) / SESSIONS;
    println!("a stopped session keeps {growth} bytes (budget {BUDGET_PER_SESSION})");
    assert!(
        growth <= BUDGET_PER_SESSION,
        "a stopped session kept {growth} bytes (budget {BUDGET_PER_SESSION}); \
         the engine is holding on to per-session state"
    );
}
