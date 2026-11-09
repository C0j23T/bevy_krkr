use std::ffi::c_void;

use super::{FAILED, guard, input, output};

/// Output size [`krkr_zlib_compress`] never exceeds for `src_len` input bytes.
#[unsafe(no_mangle)]
pub extern "C" fn krkr_zlib_bound(src_len: usize) -> usize {
    // zlib's own worst case, which also covers the empty stream's header.
    src_len + (src_len >> 12) + (src_len >> 14) + (src_len >> 25) + 13
}

/// Compresses `src` into `dst` at `level` (0 to 9; anything else selects the
/// default) and returns the byte count, or [`FAILED`] when it does not fit.
///
/// # Safety
/// `src` must hold `src_len` bytes (or be null if `src_len` is 0); `dst` must
/// hold `dst_len` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_zlib_compress(
    dst: *mut c_void,
    dst_len: usize,
    src: *const c_void,
    src_len: usize,
    level: i32,
) -> usize {
    let dst = output(dst, dst_len);
    guard(FAILED, || {
        let level = if (0..=9).contains(&level) { level as u8 } else { 6 };
        let compressed = miniz_oxide::deflate::compress_to_vec_zlib(input(src, src_len), level);
        if compressed.len() > dst.len() {
            return FAILED;
        }
        dst[..compressed.len()].copy_from_slice(&compressed);
        compressed.len()
    })
}

/// Decompresses a zlib stream into `dst` and returns the byte count, or
/// [`FAILED`] when `src` is not a stream or the output does not fit.
///
/// # Safety
/// As [`krkr_zlib_compress`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_zlib_uncompress(
    dst: *mut c_void,
    dst_len: usize,
    src: *const c_void,
    src_len: usize,
) -> usize {
    let dst = output(dst, dst_len);
    guard(FAILED, || {
        let Ok(data) = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(
            input(src, src_len),
            dst.len(),
        ) else {
            return FAILED;
        };
        dst[..data.len()].copy_from_slice(&data);
        data.len()
    })
}
