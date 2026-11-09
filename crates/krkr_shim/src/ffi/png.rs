use std::ffi::c_void;

use super::{FAILED, guard, input, output};

/// `KRKR_PNG_*` in `krkr_shim.h`.
pub const KRKR_PNG_RGBA: i32 = 0;
pub const KRKR_PNG_GRAY: i32 = 1;
pub const KRKR_PNG_INDEX: i32 = 2;

#[repr(C)]
#[derive(Default)]
pub struct KrkrPngInfo {
    pub width: u32,
    pub height: u32,
    /// IHDR bit depth, and that times the channel count.
    pub bit_depth: i32,
    pub bpp: i32,
    pub color_type: i32,
    /// Non-zero when a full-color load has to keep an alpha channel.
    pub has_alpha: i32,
    /// Non-zero when the chunk is there. `offset_unit` and `vpage_unit` are the
    /// PNG values (0 pixel, 1 micrometer); `resolution_unit` is (0 unknown,
    /// 1 meter).
    pub has_offset: i32,
    pub offset_x: i32,
    pub offset_y: i32,
    pub offset_unit: i32,
    pub has_resolution: i32,
    pub resolution_x: u32,
    pub resolution_y: u32,
    pub resolution_unit: i32,
    pub has_vpage: i32,
    pub vpage_width: u32,
    pub vpage_height: u32,
    pub vpage_unit: i32,
}

/// 0 on success, -1 when `src` is not a PNG. Fills the header and metainfo
/// fields only; the image is not decoded.
///
/// # Safety
/// `src` must hold `src_len` bytes (or be null if `src_len` is 0); `info` must
/// be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_png_info(
    src: *const c_void,
    src_len: usize,
    info: *mut KrkrPngInfo,
) -> i32 {
    let Some(info) = info.as_mut() else { return -1 };
    guard(-1, || {
        let Some(png) = crate::png::Png::parse(input(src, src_len)) else {
            return -1;
        };
        let (offset_x, offset_y, offset_unit) = png.offset.unwrap_or_default();
        let (resolution_x, resolution_y, resolution_unit) = png.resolution.unwrap_or_default();
        let (vpage_width, vpage_height, vpage_unit) = png.vpage.unwrap_or_default();
        *info = KrkrPngInfo {
            width: png.width,
            height: png.height,
            bit_depth: png.bit_depth as i32,
            bpp: png.bpp(),
            color_type: png.color_type as i32,
            has_alpha: png.has_alpha() as i32,
            has_offset: png.offset.is_some() as i32,
            offset_x,
            offset_y,
            offset_unit: offset_unit as i32,
            has_resolution: png.resolution.is_some() as i32,
            resolution_x,
            resolution_y,
            resolution_unit: resolution_unit as i32,
            has_vpage: png.vpage.is_some() as i32,
            vpage_width,
            vpage_height,
            vpage_unit: vpage_unit as i32,
        };
        0
    })
}

/// Writes the pixels in `target`'s layout (`KRKR_PNG_*`: 4 bytes, 1 byte, or 1
/// palette index per pixel) and returns the count, or [`FAILED`] when the image
/// does not decode or does not fit `dst_len`. `key` is a palette index to make
/// transparent, or -1.
///
/// # Safety
/// `src` must hold `src_len` bytes (or be null if `src_len` is 0); `dst` must
/// hold `dst_len` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_png_decode(
    src: *const c_void,
    src_len: usize,
    target: i32,
    key: i32,
    dst: *mut c_void,
    dst_len: usize,
) -> usize {
    let dst = output(dst, dst_len);
    guard(FAILED, || {
        let target = match target {
            KRKR_PNG_RGBA => crate::png::Target::Rgba,
            KRKR_PNG_GRAY => crate::png::Target::Gray,
            KRKR_PNG_INDEX => crate::png::Target::Index,
            _ => return FAILED,
        };
        let Some(pixels) = crate::png::decode(input(src, src_len), target, key) else {
            return FAILED;
        };
        if pixels.len() > dst.len() {
            return FAILED;
        }
        dst[..pixels.len()].copy_from_slice(&pixels);
        pixels.len()
    })
}

/// Encodes 24 or 32 bit pixels into a new PNG, returned through `png` and
/// `png_len` and released with [`krkr_png_free`]. 0 on success.
///
/// # Safety
/// `pixels` must hold `pixels_len` bytes; `png` and `png_len` must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_png_encode(
    pixels: *const c_void,
    pixels_len: usize,
    width: u32,
    height: u32,
    bpp: i32,
    png: *mut *mut c_void,
    png_len: *mut usize,
) -> i32 {
    let (Some(png), Some(png_len)) = (png.as_mut(), png_len.as_mut()) else {
        return -1;
    };
    guard(-1, || {
        let Some(encoded) =
            crate::png::encode(input(pixels, pixels_len), width, height, bpp as u32)
        else {
            return -1;
        };
        *png_len = encoded.len();
        *png = Box::into_raw(encoded.into_boxed_slice()) as *mut u8 as *mut c_void;
        0
    })
}

/// # Safety
/// `ptr` must come from [`krkr_png_encode`], with the `len` it reported.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_png_free(ptr: *mut c_void, len: usize) {
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
