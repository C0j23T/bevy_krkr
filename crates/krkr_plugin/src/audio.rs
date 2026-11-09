//! Sound: the engine's audio reaches the host's device.
//!
//! Kirikiroid2 decodes BGM, SE and voice itself and pushes the PCM into an
//! `iTVPSoundBuffer` (see `Kirikiroid2/src/core/environ/bevy/BevySound.cpp`),
//! which keeps the queued audio and mixes it on request. [`start`] owns the
//! output device and pulls that mix: one `krkr_audio_mix` call per device
//! callback, resampled to whatever rate the device runs at.
//!
//! It is the engine's sound path: every BGM, SE and voice a game plays is
//! decoded and mixed by Kirikiroid2, and this stream is the only place audio
//! leaves the port.
//!
//! The mix call does not touch the rest of the engine, so it is safe to make
//! from the audio thread: buffers are created, driven and released on the main
//! thread, and the port keeps them behind a lock and a shared registry.
//!
//! A game that runs its own KAG plays through `WaveSoundBuffer` from its own
//! scripts. A scenario the *host* parses only produces messages (`krkr_kag`),
//! so [`kag_bgm`] and friends forward them to `audio.tjs`, which plays them on
//! that same class.

use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use bevy::prelude::*;
use krkr_kag::events::*;
use krkr_script::Tjs2Engine;
use krkr_sys as sys;

const CHANNELS: usize = 2;

/// TJS2 source of the KAG tag bridge (`krkr_plugin/src/audio.tjs`).
const AUDIO_SCRIPT: &str = include_str!("audio.tjs");

pub struct KrkrAudioOutput {
    _stream: cpal::Stream,
}

pub fn start() -> Option<KrkrAudioOutput> {
    let host = cpal::default_host();
    let device = match host.default_output_device() {
        Some(device) => device,
        None => {
            warn!("bevy_krkr: no audio output device; the game will be silent");
            return None;
        }
    };
    let supported = match device.default_output_config() {
        Ok(config) => config,
        Err(e) => {
            warn!("bevy_krkr: audio device has no default output config: {e}");
            return None;
        }
    };
    let rate = supported.sample_rate();
    let channels = supported.channels() as usize;
    let config = supported.config();

    let mix = Arc::new(Mutex::new(Vec::<f32>::new()));
    let mix_in_callback = Arc::clone(&mix);

    let result = match supported.sample_format() {
        cpal::SampleFormat::F32 => device.build_output_stream(
            config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                fill(data, channels, rate, &mix_in_callback, |s| s);
            },
            log_stream_error,
            None,
        ),
        cpal::SampleFormat::I16 => device.build_output_stream(
            config,
            move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
                fill(data, channels, rate, &mix_in_callback, |s| {
                    (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16
                });
            },
            log_stream_error,
            None,
        ),
        cpal::SampleFormat::U16 => device.build_output_stream(
            config,
            move |data: &mut [u16], _: &cpal::OutputCallbackInfo| {
                fill(data, channels, rate, &mix_in_callback, |s| {
                    ((s.clamp(-1.0, 1.0) * 0.5 + 0.5) * u16::MAX as f32) as u16
                });
            },
            log_stream_error,
            None,
        ),
        other => {
            warn!("bevy_krkr: unsupported audio sample format {other:?}; the game will be silent");
            return None;
        }
    };

    match result {
        Ok(stream) => {
            // Some backends only start producing once the stream is played;
            // others start on build. Ignoring an "already playing" error keeps
            // both happy.
            if let Err(e) = stream.play() {
                warn!("bevy_krkr: cannot start the audio stream: {e}");
                return None;
            }
            info!("bevy_krkr: audio output {rate} Hz, {channels} channel(s)");
            Some(KrkrAudioOutput { _stream: stream })
        }
        Err(e) => {
            warn!("bevy_krkr: cannot open the audio output: {e}; the game will be silent");
            None
        }
    }
}

fn log_stream_error(error: cpal::Error) {
    warn!("bevy_krkr: audio stream error: {error}");
}

fn fill<T: Copy>(
    data: &mut [T],
    channels: usize,
    rate: u32,
    mix: &Arc<Mutex<Vec<f32>>>,
    convert: impl Fn(f32) -> T,
) {
    let frames = if channels == 0 {
        0
    } else {
        data.len() / channels
    };
    if frames == 0 {
        return;
    }
    let Ok(mut mixed) = mix.lock() else { return };
    mixed.resize(frames * CHANNELS, 0.0);
    unsafe { sys::krkr_audio_mix(mixed.as_mut_ptr(), frames as u32, rate as f64) };

    for frame in 0..frames {
        let left = mixed[frame * CHANNELS];
        let right = mixed[frame * CHANNELS + 1];
        let base = frame * channels;
        match channels {
            1 => {
                data[base] = convert((left + right) * 0.5);
            }
            _ => {
                data[base] = convert(left);
                data[base + 1] = convert(right);
                for channel in 2..channels {
                    data[base + channel] = convert(0.0);
                }
            }
        }
    }
}

// ── KAG tags ────────────────────────────────────────────────────────────────

pub fn open_kag_audio(engine: &Tjs2Engine) -> Result<(), String> {
    engine
        .exec_script(AUDIO_SCRIPT, "audio.tjs")
        .map_err(|e| e.to_string())?;
    engine
        .exec_script("global.krkrBevyAudio = new KrkrBevyAudio();", "audio.tjs")
        .map_err(|e| e.to_string())
}

pub fn close_kag_audio(engine: &Tjs2Engine) {
    const TEARDOWN: &str = "if (global.krkrBevyAudio !== void) { \
                            global.krkrBevyAudio.finalize(); global.krkrBevyAudio = void; }";
    if let Err(e) = engine.exec_script(TEARDOWN, "bevy_krkr audio teardown") {
        warn!("bevy_krkr: releasing the audio buffers failed: {e}");
    }
}

fn playing_engine<'s, 'w>(
    engine: &'s Option<NonSend<'w, Tjs2Engine>>,
) -> Option<&'s Tjs2Engine> {
    engine.as_ref().filter(|e| e.is_running()).map(|e| &**e)
}

fn call(engine: &Tjs2Engine, function: &str, args: &[&str]) {
    if let Err(e) = engine.call(function, args) {
        warn!("bevy_krkr: {function}: {e}");
    }
}

pub fn kag_bgm(
    engine: Option<NonSend<Tjs2Engine>>,
    mut play: MessageReader<PlayBgmEvent>,
    mut stop: MessageReader<StopBgmEvent>,
    mut pause: MessageReader<PauseBgmEvent>,
    mut resume: MessageReader<ResumeBgmEvent>,
    mut volume: MessageReader<BgmVolumeEvent>,
) {
    let Some(engine) = playing_engine(&engine) else {
        return;
    };
    // Stops first, then plays: `[stopbgm][playbgm]` in one frame ends with music.
    for ev in stop.read() {
        call(engine, "krkrBevyAudio.stopBgm", &[&ev.fade_ms.to_string()]);
    }
    for ev in play.read() {
        if ev.storage.is_empty() {
            warn!("bevy_krkr: [playbgm] without storage");
            continue;
        }
        call(
            engine,
            "krkrBevyAudio.playBgm",
            &[
                &ev.storage,
                if ev.looping { "true" } else { "false" },
                &amplitude(ev.volume),
                &ev.fade_ms.to_string(),
            ],
        );
    }
    for ev in volume.read() {
        call(
            engine,
            "krkrBevyAudio.setBgmVolume",
            &[&amplitude(ev.volume), &ev.fade_ms.to_string()],
        );
    }
    for _ in pause.read() {
        call(engine, "krkrBevyAudio.pauseBgm", &[]);
    }
    for _ in resume.read() {
        call(engine, "krkrBevyAudio.resumeBgm", &[]);
    }
}

pub fn kag_se(
    engine: Option<NonSend<Tjs2Engine>>,
    mut play: MessageReader<PlaySeEvent>,
    mut stop: MessageReader<StopSeEvent>,
) {
    let Some(engine) = playing_engine(&engine) else {
        return;
    };
    for ev in stop.read() {
        call(engine, "krkrBevyAudio.stopSe", &[&ev.buffer.to_string()]);
    }
    for ev in play.read() {
        if ev.storage.is_empty() {
            warn!("bevy_krkr: [playse] without storage");
            continue;
        }
        call(
            engine,
            "krkrBevyAudio.playSe",
            &[
                &ev.buffer.to_string(),
                &ev.storage,
                &amplitude(ev.volume),
                if ev.looping { "true" } else { "false" },
            ],
        );
    }
}

pub fn kag_voice(
    engine: Option<NonSend<Tjs2Engine>>,
    mut play: MessageReader<PlayVoiceEvent>,
    mut stop: MessageReader<StopVoiceEvent>,
) {
    let Some(engine) = playing_engine(&engine) else {
        return;
    };
    if stop.read().count() > 0 {
        call(engine, "krkrBevyAudio.stopVoice", &[]);
    }
    for ev in play.read() {
        if ev.storage.is_empty() {
            continue;
        }
        call(
            engine,
            "krkrBevyAudio.playVoice",
            &[&ev.storage, &amplitude(ev.volume)],
        );
    }
}

fn amplitude(volume: f32) -> String {
    format!("{}", volume.clamp(0.0, 1.0))
}
