use std::ffi::c_void;
use std::io;

use super::{FAILED, guard, input, output};

#[repr(C)]
pub struct KrkrWebpInfo {
    pub width: u32,
    pub height: u32,
    /// Non-zero when the file carries an alpha channel.
    pub has_alpha: i32,
}

/// 0 on success, -1 when `src` is not a WebP image. Fills the header fields
/// only; the image is not decoded.
///
/// # Safety
/// `src` must hold `src_len` bytes (or be null if `src_len` is 0); `info` must
/// be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_webp_info(
    src: *const c_void,
    src_len: usize,
    info: *mut KrkrWebpInfo,
) -> i32 {
    let Some(info) = info.as_mut() else { return -1 };
    guard(-1, || {
        let Ok(decoder) = image_webp::WebPDecoder::new(io::Cursor::new(input(src, src_len))) else {
            return -1;
        };
        let (width, height) = decoder.dimensions();
        *info = KrkrWebpInfo {
            width,
            height,
            has_alpha: decoder.has_alpha() as i32,
        };
        0
    })
}

/// Writes `width * height * 4` bytes of RGBA8 — the byte order of an engine
/// bitmap — and returns the count, or [`FAILED`] when the image does not decode
/// or does not fit `dst_len`.
///
/// # Safety
/// `src` must hold `src_len` bytes (or be null if `src_len` is 0); `dst` must
/// hold `dst_len` writable bytes without overlapping `src`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_webp_decode(
    src: *const c_void,
    src_len: usize,
    dst: *mut c_void,
    dst_len: usize,
) -> usize {
    let dst = output(dst, dst_len);
    guard(FAILED, || {
        let bytes = input(src, src_len);
        let Ok(mut decoder) = image_webp::WebPDecoder::new(io::Cursor::new(bytes)) else {
            return FAILED;
        };
        let (width, height) = decoder.dimensions();
        let Some(stride) = width.checked_mul(4).and_then(|row| i32::try_from(row).ok()) else {
            return FAILED;
        };
        let Some(needed) = (stride as usize).checked_mul(height as usize) else {
            return FAILED;
        };
        if needed == 0 || dst.len() < needed {
            return FAILED;
        }
        let dst = &mut dst[..needed];
        if !decoder.is_animated() {
            // SAFETY: the parsed dimensions size the non-overlapping RGBA destination.
            let decoded = unsafe {
                libwebp_sys::WebPDecodeRGBAInto(
                    bytes.as_ptr(),
                    bytes.len(),
                    dst.as_mut_ptr(),
                    dst.len(),
                    stride,
                )
            };
            return if decoded.is_null() { FAILED } else { needed };
        }
        if decoder.has_alpha() {
            return if decoder.read_image(dst).is_ok() {
                needed
            } else {
                FAILED
            };
        }
        let mut rgb = vec![0; needed / 4 * 3];
        if decoder.read_image(&mut rgb).is_err() {
            return FAILED;
        }
        for (pixel, dst) in rgb
            .as_chunks::<3>()
            .0
            .iter()
            .zip(dst.as_chunks_mut::<4>().0)
        {
            dst[..3].copy_from_slice(pixel);
            dst[3] = 0xff;
        }
        needed
    })
}
