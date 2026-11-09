use std::ffi::{c_uint, c_void};

use super::{FAILED, guard, input, output};

/// 0 on success, -1 when `src` is not a JPEG. Fills the size only.
///
/// # Safety
/// `src` must hold `src_len` bytes (or be null if `src_len` is 0); `width` and
/// `height` must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_jpeg_info(
    src: *const c_void,
    src_len: usize,
    width: *mut c_uint,
    height: *mut c_uint,
) -> i32 {
    let (Some(width), Some(height)) = (width.as_mut(), height.as_mut()) else {
        return -1;
    };
    guard(-1, || {
        let Some((w, h)) = crate::jpeg::size(input(src, src_len)) else {
            return -1;
        };
        (*width, *height) = (w, h);
        0
    })
}

/// Writes four bytes per pixel, or one when `gray` is non-zero, and returns the
/// count, or [`FAILED`] when the image does not decode or does not fit
/// `dst_len`.
///
/// # Safety
/// `src` must hold `src_len` bytes (or be null if `src_len` is 0); `dst` must
/// hold `dst_len` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_jpeg_decode(
    src: *const c_void,
    src_len: usize,
    gray: i32,
    dst: *mut c_void,
    dst_len: usize,
) -> usize {
    let dst = output(dst, dst_len);
    guard(FAILED, || {
        let Some((_, _, pixels)) = crate::jpeg::decode(input(src, src_len), gray != 0) else {
            return FAILED;
        };
        if pixels.len() > dst.len() {
            return FAILED;
        }
        dst[..pixels.len()].copy_from_slice(&pixels);
        pixels.len()
    })
}

/// Encodes four byte pixels into a new JPEG, returned through `jpeg` and
/// `jpeg_len` and released with [`krkr_jpeg_free`]. 0 on success.
///
/// # Safety
/// `pixels` must hold `pixels_len` bytes; `jpeg` and `jpeg_len` must be
/// writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_jpeg_encode(
    pixels: *const c_void,
    pixels_len: usize,
    width: u32,
    height: u32,
    quality: i32,
    subsampling: i32,
    jpeg: *mut *mut c_void,
    jpeg_len: *mut usize,
) -> i32 {
    let (Some(jpeg), Some(jpeg_len)) = (jpeg.as_mut(), jpeg_len.as_mut()) else {
        return -1;
    };
    guard(-1, || {
        let quality = quality.clamp(1, 100) as u8;
        let Some(encoded) =
            crate::jpeg::encode(input(pixels, pixels_len), width, height, quality, subsampling)
        else {
            return -1;
        };
        *jpeg_len = encoded.len();
        *jpeg = Box::into_raw(encoded.into_boxed_slice()) as *mut u8 as *mut c_void;
        0
    })
}

/// # Safety
/// `ptr` must come from [`krkr_jpeg_encode`], with the `len` it reported.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_jpeg_free(ptr: *mut c_void, len: usize) {
    if ptr.is_null() {
        return;
    }
    guard((), || {
        drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(
            ptr as *mut u8,
            len,
        )));
    });
}
