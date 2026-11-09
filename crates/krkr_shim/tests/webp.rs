//! The WebP decoder through the C ABI, the way `environ/bevy/WEBPLoader.cpp`
//! drives it: the header call, the RGBA byte order the engine bitmap expects,
//! the sentinels. The fixtures are encoded with the same crate's lossless
//! encoder, so this tests the seam rather than VP8.

use image_webp::{ColorType, WebPEncoder};
use krkr_shim::ffi::{krkr_webp_decode, krkr_webp_info, KrkrWebpInfo};

fn encode(pixels: &[u8], width: u32, height: u32, color: ColorType) -> Vec<u8> {
    let mut webp = Vec::new();
    WebPEncoder::new(&mut webp).encode(pixels, width, height, color).expect("encodes");
    webp
}

/// `(width, height, has_alpha)`, or `None` when the header call fails.
fn info(webp: &[u8]) -> Option<(u32, u32, bool)> {
    let mut info = KrkrWebpInfo { width: 0, height: 0, has_alpha: 0 };
    let status = unsafe { krkr_webp_info(webp.as_ptr().cast(), webp.len(), &mut info) };
    (status == 0).then_some((info.width, info.height, info.has_alpha != 0))
}

fn decode(webp: &[u8], width: u32, height: u32) -> Option<Vec<u8>> {
    let mut out = vec![0u8; (width * height * 4) as usize];
    let written = unsafe {
        krkr_webp_decode(webp.as_ptr().cast(), webp.len(), out.as_mut_ptr().cast(), out.len())
    };
    (written == out.len()).then_some(out)
}

#[test]
fn decodes_rgba_pixels_in_bitmap_order() {
    let pixels: Vec<u8> = vec![
        255, 0, 0, 255, // red
        0, 255, 0, 128, // half-transparent green
        0, 0, 255, 0, // fully transparent blue
        1, 2, 3, 4, //
        5, 6, 7, 8, //
        250, 251, 252, 253,
    ];
    let webp = encode(&pixels, 3, 2, ColorType::Rgba8);
    assert_eq!(info(&webp), Some((3, 2, true)));
    assert_eq!(decode(&webp, 3, 2).expect("decodes"), pixels);
}

/// A file without alpha still decodes to 4 bytes per pixel, so the caller can
/// copy rows into a 32bpp bitmap without a second case.
#[test]
fn opaque_files_get_an_opaque_alpha_byte() {
    let pixels: Vec<u8> = vec![10, 20, 30, 40, 50, 60];
    let webp = encode(&pixels, 2, 1, ColorType::Rgb8);
    assert_eq!(info(&webp), Some((2, 1, false)));
    assert_eq!(decode(&webp, 2, 1).expect("decodes"), vec![10, 20, 30, 255, 40, 50, 60, 255]);
}

#[test]
fn a_file_that_is_not_webp_fails_both_calls() {
    let garbage = b"RIFF\x00\x00\x00\x00WEBPnot really";
    assert_eq!(info(garbage), None);
    let mut out = vec![0u8; 64];
    let written = unsafe {
        krkr_webp_decode(garbage.as_ptr().cast(), garbage.len(), out.as_mut_ptr().cast(), out.len())
    };
    assert_eq!(written, usize::MAX);
}

/// The size callback sizes the engine's buffer before the decode runs, so a
/// destination that is too small has to fail rather than write past it.
#[test]
fn a_destination_that_is_too_small_fails() {
    let webp = encode(&vec![0u8; 4 * 4 * 4], 4, 4, ColorType::Rgba8);
    let mut out = vec![0u8; 4 * 4 * 4 - 1];
    let written = unsafe {
        krkr_webp_decode(webp.as_ptr().cast(), webp.len(), out.as_mut_ptr().cast(), out.len())
    };
    assert_eq!(written, usize::MAX);
}
