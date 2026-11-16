use std::{
    sync::mpsc::{self, Receiver, Sender, SyncSender, TryRecvError, TrySendError},
    thread::{self, JoinHandle},
};

use super::{
    audio,
    decoder::{Decoder, Frame},
};

enum Command {
    Seek(u64, f64),
    Select(
        u64,
        bool,
        usize,
        f64,
        SyncSender<Result<krkr_shim::video::VideoState, String>>,
    ),
    CpuFallback(SyncSender<()>),
    GpuReady,
    Close,
}

pub struct Output {
    pub frame: Option<Frame>,
}

type Message = (u64, Result<Output, String>);

pub struct Worker {
    commands: Sender<Command>,
    frames: Receiver<Message>,
    epoch: u64,
    cpu_fallback: bool,
    gpu_confirmed: bool,
    audio_id: u64,
    thread: Option<JoinHandle<()>>,
}

impl Worker {
    pub fn new(decoder: Decoder, audio_id: u64) -> Result<Self, String> {
        let (commands, receiver) = mpsc::channel();
        let (sender, frames) = mpsc::sync_channel(4);
        let thread = thread::Builder::new()
            .name("krkr-video".into())
            .spawn(move || decode(decoder, receiver, sender, audio_id))
            .map_err(|e| e.to_string())?;
        Ok(Self {
            commands,
            frames,
            epoch: 0,
            cpu_fallback: false,
            gpu_confirmed: false,
            audio_id,
            thread: Some(thread),
        })
    }

    pub fn seek(&mut self, seconds: f64) -> Result<(), String> {
        self.epoch = self
            .epoch
            .checked_add(1)
            .ok_or("video seek sequence exhausted")?;
        audio::reset(self.audio_id, self.epoch);
        self.commands
            .send(Command::Seek(self.epoch, seconds))
            .map_err(|e| e.to_string())?;
        self.thread.as_ref().unwrap().thread().unpark();
        Ok(())
    }

    pub fn select_stream(
        &mut self,
        video: bool,
        index: usize,
        seconds: f64,
    ) -> Result<krkr_shim::video::VideoState, String> {
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or("video seek sequence exhausted")?;
        let (reply, result) = mpsc::sync_channel(1);
        self.commands
            .send(Command::Select(epoch, video, index, seconds, reply))
            .map_err(|e| e.to_string())?;
        self.thread.as_ref().unwrap().thread().unpark();
        let state = result.recv().map_err(|e| e.to_string())??;
        self.epoch = epoch;
        self.cpu_fallback = false;
        self.gpu_confirmed = false;
        Ok(state)
    }

    pub fn next(&mut self) -> Result<Option<Output>, String> {
        loop {
            match self.frames.try_recv() {
                Ok((epoch, result)) => {
                    self.thread.as_ref().unwrap().thread().unpark();
                    if epoch == self.epoch {
                        let output = result?;
                        if self.cpu_fallback
                            && output
                                .frame
                                .as_ref()
                                .is_some_and(|frame| frame.pixels.is_none())
                        {
                            continue;
                        }
                        return Ok(Some(output));
                    }
                }
                Err(TryRecvError::Empty) => return Ok(None),
                Err(TryRecvError::Disconnected) => return Err("video decoder stopped".into()),
            }
        }
    }

    pub fn use_cpu_fallback(&mut self) -> Result<(), String> {
        if self.cpu_fallback {
            return Ok(());
        }
        let (reply, confirmed) = mpsc::sync_channel(1);
        self.commands
            .send(Command::CpuFallback(reply))
            .map_err(|e| e.to_string())?;
        self.thread.as_ref().unwrap().thread().unpark();
        confirmed.recv().map_err(|e| e.to_string())?;
        self.cpu_fallback = true;
        while self.frames.try_recv().is_ok() {}
        Ok(())
    }

    pub fn use_gpu_prefetch(&mut self) -> Result<(), String> {
        if self.gpu_confirmed || self.cpu_fallback {
            return Ok(());
        }
        self.commands
            .send(Command::GpuReady)
            .map_err(|e| e.to_string())?;
        self.thread.as_ref().unwrap().thread().unpark();
        self.gpu_confirmed = true;
        Ok(())
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Close);
        if let Some(thread) = self.thread.take() {
            thread.thread().unpark();
            let _ = thread.join();
        }
    }
}

fn decode(
    mut decoder: Decoder,
    commands: Receiver<Command>,
    frames: SyncSender<Message>,
    audio_id: u64,
) {
    let mut scaler = None;
    let mut software_output = !decoder.hardware;
    let mut gpu_confirmed = false;
    let mut epoch = 0;
    let mut pending: Option<Message> = None;
    let mut ended = false;
    let mut seek_target = f64::NEG_INFINITY;
    loop {
        match commands.try_recv() {
            Ok(Command::Close) | Err(TryRecvError::Disconnected) => break,
            Ok(Command::Seek(next, seconds)) => {
                epoch = next;
                seek_target = seconds;
                pending = decoder.seek(seconds).err().map(|error| (epoch, Err(error)));
                ended = pending.is_some();
            }
            Ok(Command::Select(next, video, index, seconds, reply)) => {
                let result = decoder
                    .select_stream(video, index, seconds)
                    .map(|()| decoder.state());
                if result.is_ok() {
                    epoch = next;
                    audio::reset(audio_id, epoch);
                    seek_target = seconds;
                    pending = None;
                    ended = false;
                    scaler = None;
                    software_output = !decoder.hardware;
                    gpu_confirmed = false;
                }
                let _ = reply.send(result);
            }
            Ok(Command::CpuFallback(reply)) => {
                software_output = true;
                gpu_confirmed = true;
                pending = None;
                let _ = reply.send(());
            }
            Ok(Command::GpuReady) => gpu_confirmed = true,
            Err(TryRecvError::Empty) => {}
        }
        if pending.is_none() && !ended {
            let result = decoder
                .next(&mut scaler, software_output)
                .map(|frame| Output { frame });
            audio::push(audio_id, epoch, std::mem::take(&mut decoder.audio_frames));
            ended = result
                .as_ref()
                .map_or(true, |output| output.frame.is_none());
            if result.as_ref().is_ok_and(|output| {
                output
                    .frame
                    .as_ref()
                    .is_some_and(|frame| frame.time + 0.001 < seek_target)
            }) {
                continue;
            }
            pending = Some((epoch, result));
        }
        if let Some(output) = pending.take() {
            match frames.try_send(output) {
                Ok(()) => {
                    if !software_output && !gpu_confirmed {
                        thread::park();
                    }
                    continue;
                }
                Err(TrySendError::Full(output)) => pending = Some(output),
                Err(TrySendError::Disconnected(_)) => break,
            }
        }
        thread::park();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn pcm_reaches_mixer_without_render_thread_receiving_video() {
        let id = u64::MAX - 1;
        audio::open(id);
        let decoder = Decoder::open(
            include_bytes!("../../../../tests/fixtures/video.mp4").to_vec(),
            false,
        )
        .unwrap();
        let worker = Worker::new(decoder, id).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut audible = false;
        while Instant::now() < deadline {
            audio::update(id, 0.0, true, 1.0, true);
            let mut pcm = vec![0.0; 2048];
            audio::mix(&mut pcm, 48000.0);
            if pcm.iter().any(|sample| sample.abs() > 0.01) {
                audible = true;
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        drop(worker);
        audio::close(id);
        assert!(audible, "PCM delivery depended on receiving a video frame");
    }
}
