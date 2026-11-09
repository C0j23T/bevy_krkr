//! The KAG audio tags the host parses, played on the engine's sound buffers.
//!
//! `krkr_kag` turns `[playbgm]` and friends into messages; `krkr_plugin`'s
//! `kag_bgm` / `kag_se` / `kag_voice` call the TJS object of `audio.tjs` with
//! the same arguments. This drives that object directly, the way those systems
//! do, and checks the engine's own mix
//! (`Kirikiroid2/src/core/environ/bevy/BevySound.cpp`) -- no device is opened.

use std::ffi::CString;
use std::path::PathBuf;
use std::time::Duration;

use bevy_krkr::plugin::audio::{close_kag_audio, open_kag_audio};
use bevy_krkr::script::{last_error, Tjs2Engine};
use bevy_krkr::sys;

fn data_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/game")
}

/// Mixes `frames` stereo frames and returns the peak absolute sample.
fn peak(frames: u32, rate: f64) -> f32 {
    let mut mixed = vec![0.0f32; frames as usize * 2];
    // SAFETY: the buffer holds `frames * 2` floats, which is exactly what the
    // mixer writes; it touches no other engine state, so any thread may call it.
    unsafe { sys::krkr_audio_mix(mixed.as_mut_ptr(), frames, rate) };
    mixed.into_iter().fold(0.0f32, |peak, s| peak.max(s.abs()))
}

/// Runs the engine until `stop(peak)` says so, or `ticks` run out.
fn until(ticks: u32, stop: impl Fn(f32) -> bool) -> f32 {
    let mut last = 0.0f32;
    for _ in 0..ticks {
        // SAFETY: an instance is running on this thread.
        let rc = unsafe { sys::krkr_engine_tick() };
        assert_eq!(rc, sys::KRKR_OK, "tick: {}", last_error());
        std::thread::sleep(Duration::from_millis(20));
        last = peak(1024, 44100.0);
        if stop(last) {
            break;
        }
    }
    last
}

/// The engine can be initialised once per process, so everything is one test.
#[test]
fn a_playbgm_tag_reaches_the_engine_mix() {
    let path = data_path();
    let c_path = CString::new(path.to_string_lossy().as_bytes()).expect("no NUL in the path");
    // SAFETY: called on the test thread, once for this process.
    let code = unsafe {
        sys::krkr_engine_set_screen_size(320, 240);
        sys::krkr_engine_init(c_path.as_ptr())
    };
    assert_eq!(code, sys::KRKR_OK, "krkr_engine_init: {}", last_error());
    let engine = Tjs2Engine::new().expect("Tjs2Engine::new");
    open_kag_audio(&engine).expect("audio.tjs");

    assert_eq!(peak(256, 44100.0), 0.0, "silent before anything plays");

    // `[playbgm storage="bgm" loop="true"]`: the fixture omits the extension.
    engine
        .call("krkrBevyAudio.playBgm", &["bgm", "true", "1", "0"])
        .expect("playBgm");
    let heard = until(40, |p| p > 0.0);
    assert!(heard > 0.0, "the BGM never reached the mix");

    // `[stopbgm]` has to silence it.
    engine
        .call("krkrBevyAudio.stopBgm", &["0"])
        .expect("stopBgm");
    let left = until(10, |p| p == 0.0);
    assert_eq!(left, 0.0, "a stopped BGM still plays");

    // `[fadeoutbgm time=100]`: the fade itself is the engine's, and a TJS timer
    // stops the buffer when it has run out.
    engine
        .call("krkrBevyAudio.playBgm", &["bgm", "true", "1", "0"])
        .expect("playBgm");
    assert!(until(40, |p| p > 0.0) > 0.0, "the BGM never came back");
    engine
        .call("krkrBevyAudio.stopBgm", &["100"])
        .expect("stopBgm with a fade");
    let left = until(40, |p| p == 0.0);
    assert_eq!(left, 0.0, "a faded-out BGM still plays");

    // `[playse buf=0]` with a volume: 0.5 is -6 dB, so it is audible and quieter
    // than a full-scale play would be.
    engine
        .call("krkrBevyAudio.playSe", &["0", "bgm", "0.5", "false"])
        .expect("playSe");
    let se = until(40, |p| p > 0.0);
    assert!(se > 0.0, "the SE never reached the mix");
    engine
        .call("krkrBevyAudio.stopSe", &["0"])
        .expect("stopSe");
    let left = until(20, |p| p == 0.0);
    assert_eq!(left, 0.0, "a stopped SE still plays");

    // A missing storage must be reported, not silently ignored.
    let err = engine
        .call("krkrBevyAudio.playBgm", &["no/such/file", "true", "1", "0"])
        .expect_err("a missing storage has to fail");
    assert!(
        err.to_string().contains("no such audio storage"),
        "unexpected error: {err}"
    );

    close_kag_audio(&engine);

    drop(engine);
    // SAFETY: every handle created above is released.
    unsafe { sys::krkr_engine_shutdown() };
}
