use super::{KrkrPngInfo, guard, input};
use crate::save_image::{JOBS, Job, Options, Snapshot};
use std::{
    ffi::c_void,
    sync::atomic::{AtomicBool, AtomicI32, Ordering},
};
#[repr(C)]
pub struct SaveOptions {
    pub format: u32,
    pub level: i32,
    pub meta: KrkrPngInfo,
    pub tags: *const c_void,
    pub tags_len: usize,
}
unsafe fn options(p: *const SaveOptions) -> Option<Options> {
    let p = unsafe { p.as_ref() }?;
    if p.format > 2 {
        return None;
    }
    Some(Options {
        format: p.format,
        level: p.level,
        meta: p.meta,
        tags: unsafe { input(p.tags, p.tags_len) }.to_vec(),
    })
}
unsafe fn snapshot(p: *const c_void, len: usize, w: u32, h: u32, format: u32) -> Option<Snapshot> {
    let expected = (w as usize)
        .checked_mul(h as usize)?
        .checked_mul(if format == 2 { 1 } else { 4 })?;
    if w == 0 || h == 0 || expected != len || p.is_null() {
        return None;
    }
    Some(Snapshot {
        width: w,
        height: h,
        pixels: unsafe { input(p, len) }.to_vec(),
    })
}
fn result(bytes: Vec<u8>, out: &mut *mut c_void, len: &mut usize) {
    *len = bytes.len();
    *out = Box::into_raw(bytes.into_boxed_slice()) as *mut u8 as *mut c_void;
}
/// Encodes an owned snapshot in Rust; buffers are released with krkr_png_free.
/// # Safety
/// Input buffers must be readable for their lengths and output pointers writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_save_image_encode(
    p: *const c_void,
    len: usize,
    w: u32,
    h: u32,
    opt: *const SaveOptions,
    out: *mut *mut c_void,
    out_len: *mut usize,
) -> i32 {
    guard(-1, || {
        let (Some(out), Some(out_len)) = (unsafe { out.as_mut() }, unsafe { out_len.as_mut() })
        else {
            return -1;
        };
        let Some(options) = (unsafe { options(opt) }) else {
            return -1;
        };
        let Some(image) = (unsafe { snapshot(p, len, w, h, options.format) }) else {
            return -1;
        };
        match crate::save_image::encode(
            &image,
            &options,
            &AtomicBool::new(false),
            &AtomicI32::new(0),
        ) {
            Ok(bytes) => {
                result(bytes, out, out_len);
                0
            }
            Err(_) => -1,
        }
    })
}
/// Starts a Rust worker after copying pixels and options.
/// # Safety
/// Input buffers must be readable for the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_save_image_start(
    p: *const c_void,
    len: usize,
    w: u32,
    h: u32,
    opt: *const SaveOptions,
) -> u64 {
    guard(0, || {
        let Some(options) = (unsafe { options(opt) }) else {
            return 0;
        };
        let Some(image) = (unsafe { snapshot(p, len, w, h, options.format) }) else {
            return 0;
        };
        let Ok(job) = Job::start(image, options) else {
            return 0;
        };
        let mut jobs = JOBS.lock().unwrap();
        let id = jobs.0;
        jobs.0 += 1;
        jobs.1.get_or_insert_default().insert(id, job);
        id
    })
}
/// Queries progress, returns 0 pending, 1 complete, 2 canceled, -1 error.
/// Completed output is released with krkr_png_free.
/// # Safety
/// Output pointers must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_save_image_poll(
    id: u64,
    progress: *mut i32,
    out: *mut *mut c_void,
    len: *mut usize,
) -> i32 {
    guard(-1, || {
        let (Some(progress), Some(out), Some(len)) = (
            unsafe { progress.as_mut() },
            unsafe { out.as_mut() },
            unsafe { len.as_mut() },
        ) else {
            return -1;
        };
        let mut jobs = JOBS.lock().unwrap();
        let Some(map) = jobs.1.as_mut() else {
            return -1;
        };
        let Some(job) = map.get(&id) else {
            return -1;
        };
        *progress = job.progress.load(Ordering::Acquire);
        if !job.finished() {
            return 0;
        }
        let mut job = map.remove(&id).unwrap();
        let canceled = job.canceled.load(Ordering::Acquire);
        match job.finish() {
            Ok(_) if canceled => 2,
            Ok(bytes) => {
                *progress = 100;
                result(bytes, out, len);
                1
            }
            Err(_) => -1,
        }
    })
}
/// Cancels a task; stop also joins and removes it without a completion event.
#[unsafe(no_mangle)]
pub extern "C" fn krkr_save_image_cancel(id: u64, stop: i32) {
    guard((), || {
        let mut jobs = JOBS.lock().unwrap();
        if let Some(map) = jobs.1.as_mut() {
            if stop != 0 {
                drop(map.remove(&id));
            } else if let Some(job) = map.get(&id) {
                job.canceled.store(true, Ordering::Release);
            }
        }
    });
}
