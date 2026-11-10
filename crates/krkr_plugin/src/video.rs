pub(crate) mod audio;
mod decoder;
mod worker;

use std::{collections::HashMap, sync::Mutex, time::Instant};

use bevy::prelude::*;
use krkr_shim::video::*;

use decoder::{Decoder, Frame};

static PLAYERS: Mutex<Option<Players>> = Mutex::new(None);

pub struct KrkrVideoPlugin {
    pub hardware: bool,
}

impl Default for KrkrVideoPlugin {
    fn default() -> Self {
        Self { hardware: true }
    }
}

impl Plugin for KrkrVideoPlugin {
    fn build(&self, _: &mut App) {
        initialize(self.hardware);
    }
}

pub fn initialize(hardware: bool) {
    let mut guard = PLAYERS.lock().unwrap();
    assert!(
        guard
            .as_ref()
            .is_none_or(|players| players.players.is_empty()),
        "video players are active"
    );
    *guard = Some(Players {
        players: HashMap::new(),
        next: 1,
        hardware,
    });
    drop(guard);
    krkr_shim::video::set_video_sink(dispatch);
}

pub fn mix_audio(output: &mut [f32], rate: f64) {
    if rate.is_finite() && rate > 0.0 {
        audio::mix(output, rate);
    }
}

struct Players {
    players: HashMap<u64, Player>,
    next: u64,
    hardware: bool,
}

struct Player {
    id: u64,
    decoder: worker::Worker,
    pending: Option<Frame>,
    eof: bool,
    preview: bool,
    target: u64,
    image: Option<crate::render::gpu::VideoTexture>,
    color: [f64; 4],
    state: VideoState,
    clock: Instant,
    anchor: f64,
    rate: f64,
    loop_begin: Option<f64>,
}

impl Player {
    fn position(&self) -> f64 {
        if self.state.status == 1 {
            self.anchor + self.clock.elapsed().as_secs_f64() * self.rate
        } else {
            self.anchor
        }
    }

    fn pause(&mut self) {
        self.anchor = self.position();
        self.state.position = self.anchor;
        self.state.status = 2;
    }

    fn seek(&mut self, seconds: f64) -> Result<(), String> {
        if !seconds.is_finite() || seconds < 0.0 {
            return Err("invalid video position".into());
        }
        self.decoder.seek(seconds)?;
        self.pending = None;
        self.eof = false;
        self.preview = true;
        self.anchor = seconds;
        self.clock = Instant::now();
        self.state.position = seconds;
        audio::update(self.id, seconds, self.state.status == 1, self.rate, true);
        self.state.frame = (seconds * self.state.fps) as u64;
        self.state.events = 0;
        if self.state.status == 4 {
            self.state.status = 1;
        }
        Ok(())
    }

    fn render(&mut self) -> Result<(), String> {
        if self.target != 0
            && let Some(image) = &self.image
        {
            crate::render::gpu::color_video(self.target, image, self.color)?;
            self.state.events |= UPDATED;
        }
        Ok(())
    }

    fn tick(&mut self) -> Result<(), String> {
        if (self.state.status != 1 && !self.preview) || self.target == 0 {
            return Ok(());
        }
        let position = self.position();
        let mut present = None;
        for _ in 0..16 {
            let next = match self.pending.take() {
                Some(frame) => Some(frame),
                None => {
                    let Some(output) = self.decoder.next()? else {
                        break;
                    };
                    audio::push(self.id, output.audio);
                    output.frame
                }
            };
            let Some(frame) = next else {
                self.eof = true;
                break;
            };
            if frame.time > position {
                self.pending = Some(frame);
                break;
            }
            present = Some(frame);
        }
        if self.state.status == 1
            && self.eof
            && present.is_none()
            && position >= self.state.duration
        {
            if let Some(begin) = self.loop_begin {
                self.seek(begin)?;
                return Ok(());
            }
            self.anchor = self.state.position;
            self.state.status = 4;
            self.state.events |= ENDED;
            audio::update(self.id, self.anchor, false, self.rate, false);
        }
        if let Some(frame) = present {
            crate::render::gpu::upload_video(
                &mut self.image,
                frame.width,
                frame.height,
                frame.pitch,
                frame.pixels.data(0),
            )?;
            self.render()?;
            self.state.position = frame.time;
            self.state.frame = (frame.time * self.state.fps).round().max(0.0) as u64;
            self.preview = false;
        }
        Ok(())
    }
}

unsafe fn dispatch(command: &mut VideoCommand) -> Result<(), String> {
    let mut guard = PLAYERS.lock().map_err(|_| "video players poisoned")?;
    let players = guard.as_mut().ok_or("video backend unavailable")?;
    match command.operation {
        OPEN => {
            let bytes =
                unsafe { std::slice::from_raw_parts(command.data, command.length) }.to_vec();
            let decoder = Decoder::open(bytes, players.hardware)?;
            info!(
                "video: {}x{} via {}",
                decoder.width, decoder.height, decoder.backend
            );
            let state = decoder.state();
            let id = players.next;
            players.next = id.checked_add(1).ok_or("video handles exhausted")?;
            let decoder = worker::Worker::new(decoder)?;
            audio::open(id);
            players.players.insert(
                id,
                Player {
                    id,
                    decoder,
                    pending: None,
                    eof: false,
                    preview: false,
                    target: 0,
                    image: None,
                    color: [0.0, 1.0, 0.0, 1.0],
                    state,
                    clock: Instant::now(),
                    anchor: 0.0,
                    rate: 1.0,
                    loop_begin: None,
                },
            );
            command.player = id;
            if let Some(out) = unsafe { command.state.as_mut() } {
                *out = state;
            }
            return Ok(());
        }
        CLOSE => {
            players.players.remove(&command.player);
            audio::close(command.player);
            return Ok(());
        }
        RESET => {
            for id in players.players.keys() {
                audio::close(*id);
            }
            players.players.clear();
            return Ok(());
        }
        TICK => {
            for player in players.players.values_mut() {
                if let Err(error) = player.tick() {
                    player.pause();
                    audio::update(player.id, player.anchor, false, player.rate, false);
                    return Err(error);
                }
            }
            return Ok(());
        }
        _ => {}
    }
    let player = players
        .players
        .get_mut(&command.player)
        .ok_or("unknown video player")?;
    match command.operation {
        PLAY => {
            if player.state.status == 4 {
                player.seek(0.0)?;
            }
            if player.state.status != 1 {
                player.clock = Instant::now();
                player.state.status = 1;
            }
        }
        PAUSE => player.pause(),
        STOP => {
            player.state.status = 0;
            player.seek(0.0)?;
        }
        SEEK => player.seek(command.values[0])?,
        TARGET => {
            player.target = command.target;
            if player.target == 0 {
                player.state.events = 0;
            }
            player.render()?;
        }
        COLOR => {
            if command.values.iter().any(|value| !value.is_finite()) {
                return Err("invalid video color control".into());
            }
            let bounds = [(-1.0, 1.0), (0.0, 2.0), (-180.0, 180.0), (0.0, 2.0)];
            for (value, (min, max)) in command.values.iter_mut().zip(bounds) {
                *value = value.clamp(min, max);
            }
            player.color = command.values;
            player.render()?;
        }
        RATE => {
            let rate = command.values[0];
            if !rate.is_finite() || rate <= 0.0 {
                return Err("invalid video playback rate".into());
            }
            player.anchor = player.position();
            player.clock = Instant::now();
            player.rate = rate;
        }
        AUDIO_STREAM | VIDEO_STREAM => {
            let index = command.values[0];
            if !index.is_finite() || index < 0.0 || index.fract() != 0.0 {
                return Err("invalid video/audio stream index".into());
            }
            let seconds = player.position();
            let state = player.decoder.select_stream(
                command.operation == VIDEO_STREAM,
                index as usize,
                seconds,
            )?;
            player.state.width = state.width;
            player.state.height = state.height;
            player.state.fps = state.fps;
            player.state.duration = state.duration;
            player.state.audio_stream = state.audio_stream;
            player.state.video_stream = state.video_stream;
            player.pending = None;
            player.eof = false;
            player.preview = true;
            player.image = None;
            player.anchor = seconds;
            player.clock = Instant::now();
            player.state.position = seconds;
            player.state.frame = (seconds * state.fps) as u64;
            player.state.events = 0;
            audio::update(
                player.id,
                seconds,
                player.state.status == 1,
                player.rate,
                true,
            );
            if let Some(out) = unsafe { command.state.as_mut() } {
                *out = player.state;
            }
        }
        LOOP => {
            // KRMoviePlayer loops to beginFrame at EOF. The VideoOverlay owner
            // handles endFrame via its frame events.
            let begin = command.values[0];
            if !begin.is_finite() || begin < -1.0 {
                return Err("invalid video loop start".into());
            }
            player.loop_begin = (begin >= 0.0).then_some(begin / player.state.fps);
        }
        VOLUME => audio::volume(player.id, command.values[0], command.values[1]),
        QUERY => {
            let out = unsafe { command.state.as_mut() }.ok_or("missing video state output")?;
            *out = player.state;
            out.position = player.position();
            if command.values[0] != 0.0 {
                player.state.events = 0;
            }
        }
        _ => return Err("unknown video operation".into()),
    }
    if matches!(command.operation, PLAY | PAUSE | STOP | SEEK | RATE) {
        audio::update(
            player.id,
            player.position(),
            player.state.status == 1,
            player.rate,
            false,
        );
    }
    Ok(())
}
