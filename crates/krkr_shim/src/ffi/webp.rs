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

/// `(width, height, has_alpha, pixels)`, with 3 bytes per pixel when the file
/// has no alpha.
fn webp_decode(bytes: &[u8]) -> Option<(u32, u32, bool, Vec<u8>)> {
    let mut decoder = image_webp::WebPDecoder::new(io::Cursor::new(bytes)).ok()?;
    let (width, height) = decoder.dimensions();
    let mut pixels = vec![0u8; decoder.output_buffer_size()?];
    decoder.read_image(&mut pixels).ok()?;
    Some((width, height, decoder.has_alpha(), pixels))
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
/// hold `dst_len` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_webp_decode(
    src: *const c_void,
    src_len: usize,
    dst: *mut c_void,
    dst_len: usize,
) -> usize {
    let dst = output(dst, dst_len);
    guard(FAILED, || {
        let Some((width, height, _, pixels)) = webp_decode(input(src, src_len)) else {
            return FAILED;
        };
        let Some(count) = (width as usize).checked_mul(height as usize) else {
            return FAILED;
        };
        let Some(needed) = count.checked_mul(4) else {
            return FAILED;
        };
        if count == 0 || dst.len() < needed {
            return FAILED;
        }
        let dst = &mut dst[..needed];
        match pixels.len() / count {
            // `image-webp` drops the alpha byte when the file has none.
            4 => dst.copy_from_slice(&pixels),
            3 => {
                for (pixel, dst) in pixels.as_chunks::<3>().0.iter().zip(dst.as_chunks_mut::<4>().0) {
                    dst[..3].copy_from_slice(pixel);
                    dst[3] = 0xFF;
                }
            }
            _ => return FAILED,
        }
        needed
    })
}
