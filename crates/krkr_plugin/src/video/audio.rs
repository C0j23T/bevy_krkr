use std::{
    collections::{HashMap, VecDeque},
    sync::Mutex,
    time::Instant,
};

use super::decoder::AudioFrame;

static AUDIO: Mutex<Option<HashMap<u64, Track>>> = Mutex::new(None);

struct Track {
    frames: VecDeque<AudioFrame>,
    epoch: u64,
    cursor: Option<f64>,
    position: f64,
    clock: Instant,
    playing: bool,
    rate: f64,
    volume: f32,
    balance: f32,
}

pub fn open(id: u64) {
    AUDIO.lock().unwrap().get_or_insert_default().insert(
        id,
        Track {
            frames: VecDeque::new(),
            epoch: 0,
            cursor: None,
            position: 0.0,
            clock: Instant::now(),
            playing: false,
            rate: 1.0,
            volume: 1.0,
            balance: 0.0,
        },
    );
}

pub fn close(id: u64) {
    if let Some(tracks) = AUDIO.lock().unwrap().as_mut() {
        tracks.remove(&id);
    }
}

pub fn update(id: u64, position: f64, playing: bool, rate: f64, seek: bool) {
    let mut guard = AUDIO.lock().unwrap();
    let Some(track) = guard.as_mut().and_then(|tracks| tracks.get_mut(&id)) else {
        return;
    };
    if seek || track.playing != playing || track.rate != rate {
        track.cursor = None;
    }
    track.position = position;
    track.clock = Instant::now();
    track.playing = playing;
    track.rate = rate;
}

pub fn volume(id: u64, volume: f64, balance: f64) {
    let mut guard = AUDIO.lock().unwrap();
    if let Some(track) = guard.as_mut().and_then(|tracks| tracks.get_mut(&id)) {
        track.volume = volume.clamp(0.0, 1.0) as f32;
        track.balance = balance.clamp(-1.0, 1.0) as f32;
    }
}

pub fn reset(id: u64, epoch: u64) {
    let mut guard = AUDIO.lock().unwrap();
    if let Some(track) = guard.as_mut().and_then(|tracks| tracks.get_mut(&id)) {
        track.epoch = epoch;
        track.frames.clear();
    }
}

pub fn push(id: u64, epoch: u64, frames: Vec<AudioFrame>) {
    let mut guard = AUDIO.lock().unwrap();
    if let Some(track) = guard.as_mut().and_then(|tracks| tracks.get_mut(&id)) {
        if track.epoch != epoch {
            return;
        }
        track.frames.extend(frames);
        while track.frames.len() > 128 {
            track.frames.pop_front();
        }
    }
}

pub fn mix(output: &mut [f32], rate: f64) {
    let Ok(mut guard) = AUDIO.lock() else {
        return;
    };
    let Some(tracks) = guard.as_mut() else {
        return;
    };
    for track in tracks.values_mut().filter(|track| track.playing) {
        mix_track(track, output, rate);
    }
}

fn mix_track(track: &mut Track, output: &mut [f32], rate: f64) {
    let start = track
        .cursor
        .unwrap_or_else(|| track.position + track.clock.elapsed().as_secs_f64() * track.rate);
    track.cursor = Some(start + output.len() as f64 / 2.0 / rate * track.rate);
    for (index, sample) in output.as_chunks_mut::<2>().0.iter_mut().enumerate() {
        let time = start + index as f64 / rate * track.rate;
        while track.frames.front().is_some_and(|frame| {
            frame.time + frame.samples.len() as f64 / 96000.0 <= time + 1e-7 / 48000.0
        }) {
            track.frames.pop_front();
        }
        let Some(frame) = track.frames.front() else {
            break;
        };
        if time + 1e-7 / 48000.0 < frame.time {
            continue;
        }
        let position = ((time - frame.time) * 48000.0).max(0.0) + 1e-7;
        let offset = position as usize * 2;
        if let Some(pcm) = frame.samples.get(offset..offset + 2) {
            let next = frame
                .samples
                .get(offset + 2..offset + 4)
                .or_else(|| {
                    track
                        .frames
                        .get(1)
                        .filter(|next| {
                            (next.time - frame.time - frame.samples.len() as f64 / 96000.0).abs()
                                < 0.5 / 48000.0
                        })
                        .and_then(|next| next.samples.get(..2))
                })
                .unwrap_or(pcm);
            let fraction = position.fract() as f32;
            let left = pcm[0] + (next[0] - pcm[0]) * fraction;
            let right = pcm[1] + (next[1] - pcm[1]) * fraction;
            sample[0] += left * track.volume * (1.0 - track.balance.max(0.0));
            sample[1] += right * track.volume * (1.0 + track.balance.min(0.0));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track() -> Track {
        Track {
            frames: VecDeque::from([AudioFrame {
                time: 0.0,
                samples: (0..4800)
                    .flat_map(|index| [index as f32 / 4800.0; 2])
                    .collect(),
            }]),
            epoch: 0,
            cursor: Some(0.0),
            position: 0.0,
            clock: Instant::now(),
            playing: true,
            rate: 1.0,
            volume: 1.0,
            balance: 0.0,
        }
    }

    #[test]
    fn callback_jitter_does_not_skip_or_repeat_pcm() {
        let mut track = track();
        let samples = track.frames.pop_front().unwrap().samples;
        for (index, samples) in samples.chunks(194).enumerate() {
            track.frames.push_back(AudioFrame {
                time: index as f64 * 97.0 / 48000.0,
                samples: samples.to_vec(),
            });
        }
        let mut output = Vec::new();
        for count in [127, 511, 32, 1024, 256] {
            track.clock = Instant::now() - std::time::Duration::from_secs(10);
            let mut block = vec![0.0; count * 2];
            mix_track(&mut track, &mut block, 48000.0);
            output.extend(block);
        }
        for (index, pair) in output.chunks_exact(2).enumerate() {
            let expected = index as f32 / 4800.0;
            assert!((pair[0] - expected).abs() < 0.00001, "{index}: {pair:?}");
            assert_eq!(pair[0], pair[1]);
        }
    }

    #[test]
    fn rounded_frame_boundary_does_not_insert_silence() {
        let mut track = track();
        track.frames = VecDeque::from([
            AudioFrame {
                time: 0.0,
                samples: vec![0.5; 2048],
            },
            AudioFrame {
                time: 1024.0 / 48000.0,
                samples: vec![0.5; 2048],
            },
        ]);
        for offset in [-1e-16, 1e-16] {
            track.cursor = Some(1024.0 / 48000.0 + offset);
            let mut output = [0.0; 2];
            mix_track(&mut track, &mut output, 48000.0);
            assert_eq!(output, [0.5; 2]);
        }
    }

    #[test]
    fn device_resampling_interpolates_across_pcm_frame_boundaries() {
        let mut track = track();
        let samples = track.frames.pop_front().unwrap().samples;
        for (index, samples) in samples.chunks(200).enumerate() {
            track.frames.push_back(AudioFrame {
                time: index as f64 * 100.0 / 48000.0,
                samples: samples.to_vec(),
            });
        }
        let mut output = vec![0.0; 2400];
        mix_track(&mut track, &mut output, 44100.0);
        for (index, pair) in output.chunks_exact(2).enumerate() {
            let expected = index as f32 / 44100.0 * 10.0;
            assert!((pair[0] - expected).abs() < 1e-6, "{index}: {pair:?}");
            assert_eq!(pair[0], pair[1]);
        }
    }

    #[test]
    fn seek_rejects_pcm_from_previous_epoch() {
        let id = u64::MAX;
        open(id);
        reset(id, 1);
        push(
            id,
            0,
            vec![AudioFrame {
                time: 0.0,
                samples: vec![1.0; 32],
            }],
        );
        push(
            id,
            1,
            vec![AudioFrame {
                time: 5.0,
                samples: vec![0.5; 32],
            }],
        );
        {
            let guard = AUDIO.lock().unwrap();
            let track = &guard.as_ref().unwrap()[&id];
            assert_eq!(track.frames.len(), 1);
            assert_eq!(track.frames[0].time, 5.0);
        }
        close(id);
    }
}
