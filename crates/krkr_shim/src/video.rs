use std::sync::RwLock;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct VideoState {
    pub width: u32,
    pub height: u32,
    pub duration: f64,
    pub position: f64,
    pub fps: f64,
    pub status: u32,
    pub events: u32,
    pub frame: u64,
    pub audio_streams: u32,
    pub video_streams: u32,
    pub audio_stream: i32,
    pub video_stream: i32,
}

#[repr(C)]
pub struct VideoCommand {
    pub operation: u32,
    pub player: u64,
    pub data: *const u8,
    pub length: usize,
    pub target: u64,
    pub values: [f64; 4],
    pub state: *mut VideoState,
    pub error: *mut u8,
    pub error_capacity: usize,
}

pub const OPEN: u32 = 0;
pub const PLAY: u32 = 1;
pub const PAUSE: u32 = 2;
pub const STOP: u32 = 3;
pub const SEEK: u32 = 4;
pub const CLOSE: u32 = 5;
pub const QUERY: u32 = 6;
pub const TARGET: u32 = 7;
pub const RATE: u32 = 8;
pub const VOLUME: u32 = 9;
pub const TICK: u32 = 10;
pub const RESET: u32 = 11;
pub const COLOR: u32 = 12;
pub const AUDIO_STREAM: u32 = 13;
pub const VIDEO_STREAM: u32 = 14;
pub const LOOP: u32 = 15;
pub const UPDATED: u32 = 1;
pub const ENDED: u32 = 2;

pub type VideoSink = unsafe fn(&mut VideoCommand) -> Result<(), String>;
static SINK: RwLock<Option<VideoSink>> = RwLock::new(None);

pub fn set_video_sink(sink: VideoSink) {
    *SINK.write().unwrap() = Some(sink);
}

/// # Safety
/// All pointers must remain valid for this call. The sink must copy any input
/// it retains and must not unwind across the ABI.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_video_dispatch(command: *mut VideoCommand) -> i32 {
    let result = std::panic::catch_unwind(|| {
        let command = unsafe { command.as_mut() }.ok_or("null video command")?;
        if command.operation == OPEN
            && (command.data.is_null()
                || command.length == 0
                || command.length > isize::MAX as usize)
        {
            return Err("empty video input".to_owned());
        }
        let sink = *SINK.read().map_err(|_| "video sink poisoned")?;
        match sink {
            Some(sink) => unsafe { sink(command) },
            None if matches!(command.operation, TICK | RESET) => Ok(()),
            None => Err("video backend is not initialized".to_owned()),
        }
    });
    match result {
        Ok(Ok(())) => 0,
        Ok(Err(error)) => {
            if let Some(command) = unsafe { command.as_ref() }
                && !command.error.is_null()
                && command.error_capacity > 0
            {
                let length = error.len().min(command.error_capacity - 1);
                unsafe {
                    std::ptr::copy_nonoverlapping(error.as_ptr(), command.error, length);
                    *command.error.add(length) = 0;
                }
            }
            eprintln!("bevy_krkr: {error}");
            -1
        }
        Err(_) => -1,
    }
}
