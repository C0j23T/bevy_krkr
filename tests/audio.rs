//! The engine's own sound path reaches the host as PCM.
//!
//! A game's KAG scripts play BGM and SE through `WaveSoundBuffer`, which is the
//! engine's decoder plus an `iTVPSoundBuffer`. The port implements that buffer
//! and mixes it for the host (`Kirikiroid2/src/core/environ/bevy/BevySound.cpp`,
//! `krkr_audio_mix`); this checks audio actually comes out, and stops when the
//! script stops it.

use std::ffi::CString;
use std::path::PathBuf;
use std::time::Duration;

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

/// The engine can be initialised once per process, so everything is one test.
#[test]
fn a_wave_sound_buffer_reaches_the_host_mix() {
    let path = data_path();
    let c_path = CString::new(path.to_string_lossy().as_bytes()).expect("no NUL in the path");
    // SAFETY: called on the test thread, once for this process.
    let code = unsafe {
        sys::krkr_engine_set_screen_size(320, 240);
        sys::krkr_engine_init(c_path.as_ptr())
    };
    assert_eq!(code, sys::KRKR_OK, "krkr_engine_init: {}", last_error());
    let engine = Tjs2Engine::new().expect("Tjs2Engine::new");

    // Nothing played yet: the mix is silent.
    assert_eq!(peak(256, 44100.0), 0.0, "silent before anything plays");

    engine
        .exec_script(
            r#"
                global.owner = %[];
                global.bgm = new WaveSoundBuffer(global.owner);
                bgm.open("bgm.wav");
                bgm.looping = true;
                bgm.volume = 100000;
                bgm.play();
            "#,
            "audio test",
        )
        .expect("open and play a wav");

    // The engine decodes on its own timer thread and hands the PCM to the sound
    // buffer, so this takes a few ticks.
    let mut heard = 0.0f32;
    for _ in 0..40 {
        // SAFETY: an instance is running on this thread.
        let rc = unsafe { sys::krkr_engine_tick() };
        assert_eq!(rc, sys::KRKR_OK, "tick: {}", last_error());
        std::thread::sleep(Duration::from_millis(20));
        heard = heard.max(peak(1024, 44100.0));
        if heard > 0.0 {
            break;
        }
    }
    assert!(heard > 0.0, "the wav never reached the mix");

    // KAG's stop has to silence it again.
    engine.exec_script("bgm.stop();", "audio test").unwrap();
    for _ in 0..10 {
        // SAFETY: as above.
        unsafe { sys::krkr_engine_tick() };
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(peak(1024, 44100.0), 0.0, "a stopped buffer still plays");

    drop(engine);
    // SAFETY: every handle created above is released.
    unsafe { sys::krkr_engine_shutdown() };
}
