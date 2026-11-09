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

#[no_mangle]
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
