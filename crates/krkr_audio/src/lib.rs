//! `krkr_audio` – BGM / SE / voice playback through `bevy_kira_audio`.
//!
//! Each KAG audio category has its own `bevy_kira_audio` channel, so volume,
//! pause and resume apply per category:
//!
//! | resource                    | KAG tags                                        |
//! |-----------------------------|-------------------------------------------------|
//! | `AudioChannel<BgmChannel>`  | playbgm, fadeinbgm, stopbgm, fadeoutbgm, pausebgm, resumebgm, bgmopt, fadebgm |
//! | `AudioChannel<SeChannel>`   | playse, stopse                                  |
//! | `AudioChannel<VoiceChannel>`| playvoice, stopvoice                            |
//!
//! Playback, fades, looping and volume all use `bevy_kira_audio`; there is no
//! second mixer. The C ABI does not expose Kirikiroid2's PCM output, so a KAG
//! `storage` name is resolved to a Bevy asset path ([`KrkrAudioSettings`])
//! and loaded with `AssetServer::load`.
//!
//! Bevy's own `bevy_audio` must stay disabled; the workspace builds Bevy
//! without its `audio` feature.
//!
//! This is the path for KAG tags the *host* parses (`krkr_kag`). A game that
//! runs its own KAG (`system/bgm.tjs` and friends) never produces these events:
//! it plays through the engine's `WaveSoundBuffer`, which the port mixes for the
//! device itself (`crates/krkr_plugin/src/audio.rs`). Both can run at once.

use std::collections::HashMap;
use std::time::Duration;

use bevy::prelude::*;
use bevy_kira_audio::prelude::{
    AudioApp, AudioChannel, AudioControl, AudioInstance, AudioPlugin, AudioSource, AudioTween,
    Decibels,
};
use krkr_kag::events::*;

/// BGM channel marker.
#[derive(Resource, Default)]
pub struct BgmChannel;

/// Sound-effect channel marker.
#[derive(Resource, Default)]
pub struct SeChannel;

/// Voice channel marker.
#[derive(Resource, Default)]
pub struct VoiceChannel;

/// How KAG `storage` names map to Bevy asset paths.
#[derive(Resource, Debug, Clone)]
pub struct KrkrAudioSettings {
    /// Prefix joined to every storage name, e.g. `"game/"`.
    pub storage_root: String,
    /// Extension appended when the storage name has none (KAG usually omits it).
    pub default_extension: Option<String>,
}

impl Default for KrkrAudioSettings {
    fn default() -> Self {
        Self {
            storage_root: String::new(),
            default_extension: Some("ogg".into()),
        }
    }
}

impl KrkrAudioSettings {
    pub fn resolve(&self, storage: &str) -> String {
        let has_ext = std::path::Path::new(storage).extension().is_some();
        match (&self.default_extension, has_ext) {
            (Some(ext), false) => format!("{}{storage}.{ext}", self.storage_root),
            _ => format!("{}{storage}", self.storage_root),
        }
    }
}

/// Instances currently playing per KAG SE buffer, so `[stopse buf=n]` stops
/// only buffer `n`.
#[derive(Resource, Default)]
pub struct SeBuffers(pub HashMap<u8, Handle<AudioInstance>>);

/// Linear amplitude (0.0–1.0) → decibels for kira.
pub fn amplitude_to_db(amplitude: f32) -> Decibels {
    if amplitude <= 0.001 {
        Decibels::SILENCE
    } else {
        Decibels(20.0 * amplitude.min(1.0).log10())
    }
}

fn tween(ms: u32) -> AudioTween {
    if ms == 0 {
        AudioTween::default()
    } else {
        AudioTween::linear(Duration::from_millis(ms.into()))
    }
}

/// System set for audio work in `Update`. Runs after `krkr_kag::KagSet`.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct KrkrAudioSet;

#[derive(Default)]
pub struct KrkrAudioPlugin;

impl Plugin for KrkrAudioPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<AudioPlugin>() {
            app.add_plugins(AudioPlugin);
        }
        app.add_audio_channel::<BgmChannel>()
            .add_audio_channel::<SeChannel>()
            .add_audio_channel::<VoiceChannel>()
            .init_resource::<KrkrAudioSettings>()
            .init_resource::<SeBuffers>()
            .configure_sets(Update, KrkrAudioSet.after(krkr_kag::KagSet))
            .add_systems(
                Update,
                (bgm_system, se_system, voice_system).in_set(KrkrAudioSet),
            );
    }
}

#[allow(clippy::too_many_arguments)]
fn bgm_system(
    mut play: MessageReader<PlayBgmEvent>,
    mut stop: MessageReader<StopBgmEvent>,
    mut pause: MessageReader<PauseBgmEvent>,
    mut resume: MessageReader<ResumeBgmEvent>,
    mut volume: MessageReader<BgmVolumeEvent>,
    bgm: Res<AudioChannel<BgmChannel>>,
    settings: Res<KrkrAudioSettings>,
    assets: Res<AssetServer>,
) {
    // Order within a frame: stop, then play, so `[stopbgm][playbgm]` works.
    for ev in stop.read() {
        bgm.stop().fade_out(tween(ev.fade_ms));
    }
    for ev in play.read() {
        if ev.storage.is_empty() {
            warn!("krkr_audio: [playbgm] without storage");
            continue;
        }
        // KAG plays one BGM at a time.
        bgm.stop();
        let source: Handle<AudioSource> = assets.load(settings.resolve(&ev.storage));
        let mut cmd = bgm.play(source);
        cmd.with_volume(amplitude_to_db(ev.volume));
        if ev.looping {
            cmd.looped();
        }
        if ev.fade_ms > 0 {
            cmd.fade_in(tween(ev.fade_ms));
        }
    }
    for ev in volume.read() {
        bgm.set_volume(amplitude_to_db(ev.volume))
            .fade_in(tween(ev.fade_ms));
    }
    for _ in pause.read() {
        bgm.pause();
    }
    for _ in resume.read() {
        bgm.resume();
    }
}

fn se_system(
    mut play: MessageReader<PlaySeEvent>,
    mut stop: MessageReader<StopSeEvent>,
    se: Res<AudioChannel<SeChannel>>,
    settings: Res<KrkrAudioSettings>,
    assets: Res<AssetServer>,
    mut buffers: ResMut<SeBuffers>,
    mut instances: ResMut<Assets<AudioInstance>>,
) {
    for ev in stop.read() {
        stop_buffer(&mut buffers, &mut instances, ev.buffer);
    }
    for ev in play.read() {
        if ev.storage.is_empty() {
            warn!("krkr_audio: [playse] without storage");
            continue;
        }
        stop_buffer(&mut buffers, &mut instances, ev.buffer);
        let source: Handle<AudioSource> = assets.load(settings.resolve(&ev.storage));
        let mut cmd = se.play(source);
        cmd.with_volume(amplitude_to_db(ev.volume));
        if ev.looping {
            cmd.looped();
        }
        buffers.0.insert(ev.buffer, cmd.handle());
    }
}

fn stop_buffer(buffers: &mut SeBuffers, instances: &mut Assets<AudioInstance>, buffer: u8) {
    if let Some(handle) = buffers.0.remove(&buffer) {
        if let Some(mut instance) = instances.get_mut(&handle) {
            instance.stop(AudioTween::default());
        }
    }
}

fn voice_system(
    mut play: MessageReader<PlayVoiceEvent>,
    mut stop: MessageReader<StopVoiceEvent>,
    voice: Res<AudioChannel<VoiceChannel>>,
    settings: Res<KrkrAudioSettings>,
    assets: Res<AssetServer>,
) {
    if stop.read().count() > 0 {
        voice.stop();
    }
    for ev in play.read() {
        if ev.storage.is_empty() {
            continue;
        }
        // A new voice line interrupts the previous one.
        voice.stop();
        let source: Handle<AudioSource> = assets.load(settings.resolve(&ev.storage));
        voice.play(source).with_volume(amplitude_to_db(ev.volume));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_resolution() {
        let s = KrkrAudioSettings {
            storage_root: "game/".into(),
            default_extension: Some("ogg".into()),
        };
        assert_eq!(s.resolve("bgm01"), "game/bgm01.ogg");
        assert_eq!(s.resolve("se/click.wav"), "game/se/click.wav");
    }

    #[test]
    fn volume_conversion() {
        assert_eq!(amplitude_to_db(1.0).0, 0.0);
        assert_eq!(amplitude_to_db(0.0), Decibels::SILENCE);
        assert!((amplitude_to_db(0.5).0 + 6.02).abs() < 0.01);
    }
}
