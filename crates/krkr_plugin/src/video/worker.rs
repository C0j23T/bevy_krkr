use std::{
    sync::mpsc::{self, Receiver, Sender, SyncSender, TryRecvError, TrySendError},
    thread::{self, JoinHandle},
};

use super::decoder::{AudioFrame, Decoder, Frame};

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
    pub audio: Vec<AudioFrame>,
}

type Message = (u64, Result<Output, String>);

pub struct Worker {
    commands: Sender<Command>,
    frames: Receiver<Message>,
    epoch: u64,
    cpu_fallback: bool,
    gpu_confirmed: bool,
    pending_audio: Vec<AudioFrame>,
    thread: Option<JoinHandle<()>>,
}

impl Worker {
    pub fn new(decoder: Decoder) -> Result<Self, String> {
        let (commands, receiver) = mpsc::channel();
        let (sender, frames) = mpsc::sync_channel(4);
        let thread = thread::Builder::new()
            .name("krkr-video".into())
            .spawn(move || decode(decoder, receiver, sender))
            .map_err(|e| e.to_string())?;
        Ok(Self {
            commands,
            frames,
            epoch: 0,
            cpu_fallback: false,
            gpu_confirmed: false,
            pending_audio: Vec::new(),
            thread: Some(thread),
        })
    }

    pub fn seek(&mut self, seconds: f64) -> Result<(), String> {
        self.epoch = self
            .epoch
            .checked_add(1)
            .ok_or("video seek sequence exhausted")?;
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
        self.pending_audio.clear();
        Ok(state)
    }

    pub fn next(&mut self) -> Result<Option<Output>, String> {
        loop {
            match self.frames.try_recv() {
                Ok((epoch, result)) => {
                    self.thread.as_ref().unwrap().thread().unpark();
                    if epoch == self.epoch {
                        let mut output = result?;
                        if self.cpu_fallback
                            && output
                                .frame
                                .as_ref()
                                .is_some_and(|frame| frame.pixels.is_none())
                        {
                            self.pending_audio.append(&mut output.audio);
                            continue;
                        }
                        if !self.pending_audio.is_empty() {
                            self.pending_audio.append(&mut output.audio);
                            output.audio = std::mem::take(&mut self.pending_audio);
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
        while let Ok((epoch, result)) = self.frames.try_recv() {
            if epoch == self.epoch
                && let Ok(mut output) = result
            {
                self.pending_audio.append(&mut output.audio);
            }
        }
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

fn decode(mut decoder: Decoder, commands: Receiver<Command>, frames: SyncSender<Message>) {
    let mut scaler = None;
    let mut software_output = !decoder.hardware;
    let mut gpu_confirmed = false;
    let mut epoch = 0;
    let mut pending: Option<Message> = None;
    let mut pending_audio = Vec::new();
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
                if let Some((_, Ok(mut output))) = pending.take() {
                    pending_audio.append(&mut output.audio);
                }
                let _ = reply.send(());
            }
            Ok(Command::GpuReady) => gpu_confirmed = true,
            Err(TryRecvError::Empty) => {}
        }
        if pending.is_none() && !ended {
            let result = decoder.next(&mut scaler, software_output).map(|frame| {
                pending_audio.append(&mut decoder.audio_frames);
                Output {
                    frame,
                    audio: std::mem::take(&mut pending_audio),
                }
            });
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
