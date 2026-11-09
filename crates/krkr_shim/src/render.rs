use std::ffi::{c_char, c_void};
use std::sync::RwLock;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RenderCommand {
    pub operation: u32,
    pub format: u32,
    pub target: u64,
    pub sources: [u64; 3],
    pub rect: [i32; 4],
    pub source_rects: [[f32; 4]; 3],
    pub parameters: [f32; 16],
    pub method: *const c_char,
    pub data: *mut c_void,
    pub pitch: i32,
    pub geometry: u32,
    pub points: [[f32; 8]; 4],
}

pub type RenderSink = unsafe fn(&mut RenderCommand) -> Result<(), String>;
static SINK: RwLock<Option<RenderSink>> = RwLock::new(None);

pub fn set_render_sink(sink: RenderSink) {
    *SINK.write().unwrap() = Some(sink);
}

/// `int krkr_render_dispatch(TVPBevyRenderCommand*)`: hands one command to the
/// registered [`RenderSink`]. Returns `0` on success and `-1` when the command
/// is rejected (the reason goes to stderr) or the sink panicked.
///
/// The pointers inside `command` only have to live until this returns; the sink
/// copies whatever it needs.
///
/// # Safety
/// - `command` must point to an initialized, properly aligned `RenderCommand`,
///   laid out exactly like `TVPBevyRenderCommand` in `BevyRender.h`. Null is
///   tolerated and reported as a failure.
/// - If `method` is non-null it must be a NUL-terminated C string, and if
///   `data` is non-null it must cover the bitmap described by `rect`, `format`
///   and `pitch` (`pitch` is signed and may be negative for bottom-up rows).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_render_dispatch(command: *mut RenderCommand) -> i32 {
    let result = std::panic::catch_unwind(|| {
        let command = unsafe { command.as_mut() }.ok_or("null render command")?;
        let sink = SINK
            .read()
            .map_err(|_| "render sink poisoned")?
            .ok_or("Bevy renderer is not initialized")?;
        unsafe { sink(command) }
    });
    match result {
        Ok(Ok(())) => 0,
        Ok(Err(error)) => {
            eprintln!("bevy_krkr: {error}");
            -1
        }
        Err(_) => -1,
    }
}
