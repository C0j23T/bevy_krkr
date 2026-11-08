//! Sound output: the engine's `WaveSoundBuffer` reaches the host's device.
//!
//! Kirikiroid2's sound buffers are a PCM sink: `tTJSNI_WaveSoundBuffer`
//! decodes with the engine's decoders and pushes the result in, and on Windows
//! a DirectSound buffer would play it. This build has no mixer, so the port
//! keeps the queued PCM and mixes it
//! (`Kirikiroid2/src/core/environ/bevy/BevySound.cpp`); the host pulls the mix
//! once per device callback and writes it out.
//!
//! KAG's scripts build their audio on `WaveSoundBuffer` (BGM, SE and voice in
//! `system/bgm.tjs`, `se.tjs`), so this is the only path a game that runs its
//! own UI has to sound. `krkr_audio` (bevy_kira_audio) covers the host-parsed
//! KAG tags instead.

use super::KirikiridModule::Audio;
use super::{p, FnDecl};

pub const FUNCTIONS: &[FnDecl] = &[FnDecl {
    module: Audio,
    name: "krkr_audio_mix",
    doc: "Mix every sound buffer the engine is playing into `out`, as \
              `frames` interleaved stereo float frames, resampling to `out_rate` Hz. \
              `out` is overwritten, not accumulated onto, and is silent when nothing \
              plays. Safe to call from any thread; it does not touch the rest of the engine, \
              so the host's audio callback can call it while scripts are idle.",
    c_ret: "void",
    rust_ret: "",
    params: &[
        p("out", "float*", "*mut f32"),
        p("frames", "unsigned int", "c_uint"),
        p("out_rate", "double", "f64"),
    ],
    cpp_target: "TVPBevyMixAudio (environ/bevy/BevySound.cpp)",
    cpp_body: r#"
    if (!out) return;
    TVPBevyMixAudio(out, frames, out_rate);"#,
}];
