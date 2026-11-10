use std::{
    collections::{HashMap, VecDeque},
    sync::Mutex,
    time::Instant,
};

use super::decoder::AudioFrame;

static AUDIO: Mutex<Option<HashMap<u64, Track>>> = Mutex::new(None);

struct Track {
    frames: VecDeque<AudioFrame>,
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
    track.position = position;
    track.clock = Instant::now();
    track.playing = playing;
    track.rate = rate;
    if seek {
        track.frames.clear();
    }
}

pub fn volume(id: u64, volume: f64, balance: f64) {
    let mut guard = AUDIO.lock().unwrap();
    if let Some(track) = guard.as_mut().and_then(|tracks| tracks.get_mut(&id)) {
        track.volume = volume.clamp(0.0, 1.0) as f32;
        track.balance = balance.clamp(-1.0, 1.0) as f32;
    }
}

pub fn push(id: u64, frames: Vec<AudioFrame>) {
    let mut guard = AUDIO.lock().unwrap();
    if let Some(track) = guard.as_mut().and_then(|tracks| tracks.get_mut(&id)) {
        track.frames.extend(frames);
        while track.frames.len() > 128 {
            track.frames.pop_front();
        }
    }
}

pub fn mix(output: &mut [f32], rate: f64) {
    let Ok(mut guard) = AUDIO.try_lock() else {
        return;
    };
    let Some(tracks) = guard.as_mut() else {
        return;
    };
    for track in tracks.values_mut().filter(|track| track.playing) {
        let start = track.position + track.clock.elapsed().as_secs_f64() * track.rate;
        for (index, sample) in output.as_chunks_mut::<2>().0.iter_mut().enumerate() {
            let time = start + index as f64 / rate * track.rate;
            while track
                .frames
                .front()
                .is_some_and(|frame| frame.time + frame.samples.len() as f64 / 96000.0 <= time)
            {
                track.frames.pop_front();
            }
            let Some(frame) = track.frames.front() else {
                break;
            };
            if time < frame.time {
                continue;
            }
            let offset = ((time - frame.time) * 48000.0) as usize * 2;
            if let Some(pcm) = frame.samples.get(offset..offset + 2) {
                sample[0] += pcm[0] * track.volume * (1.0 - track.balance.max(0.0));
                sample[1] += pcm[1] * track.volume * (1.0 + track.balance.min(0.0));
            }
        }
    }
}
