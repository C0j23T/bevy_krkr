//! The PNG codec through the C ABI, the way `visual/LoadPNG.cpp` drives it: the
//! header call, the row layouts the engine bitmap asks for, the metainfo
//! chunks, and the encoder. Fixtures for the layouts the encoder cannot write
//! are built with the same crate.

use krkr_shim::ffi::{
    KRKR_PNG_GRAY, KRKR_PNG_INDEX, KRKR_PNG_RGBA, KrkrPngInfo, krkr_png_decode, krkr_png_encode,
    krkr_png_free, krkr_png_info,
};
use png::{BitDepth, ColorType, Encoder};

fn info(png: &[u8]) -> Option<KrkrPngInfo> {
    let mut info = KrkrPngInfo::default();
    let status = unsafe { krkr_png_info(png.as_ptr().cast(), png.len(), &mut info) };
    (status == 0).then_some(info)
}

/// Decodes into a buffer of `len` bytes, which the engine sizes from the target
/// layout; any other byte count is a failure.
fn decode(png: &[u8], target: i32, key: i32, len: usize) -> Option<Vec<u8>> {
    let mut out = vec![0u8; len];
    let written = unsafe {
        krkr_png_decode(
            png.as_ptr().cast(),
            png.len(),
            target,
            key,
            out.as_mut_ptr().cast(),
            out.len(),
        )
    };
    (written == len).then_some(out)
}

fn encode(pixels: &[u8], width: u32, height: u32, bpp: i32) -> Vec<u8> {
    let mut png = std::ptr::null_mut();
    let mut len = 0;
    let status = unsafe {
        krkr_png_encode(
            pixels.as_ptr().cast(),
            pixels.len(),
            width,
            height,
            bpp,
            &mut png,
            &mut len,
        )
    };
    assert_eq!(status, 0, "encodes");
    let out = unsafe { std::slice::from_raw_parts(png as *const u8, len) }.to_vec();
    unsafe { krkr_png_free(png, len) };
    out
}

fn fixture(width: u32, height: u32, color: ColorType, depth: BitDepth, data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut encoder = Encoder::new(&mut out, width, height);
    encoder.set_color(color);
    encoder.set_depth(depth);
    let mut writer = encoder.write_header().expect("writes a header");
    writer.write_image_data(data).expect("encodes");
    writer.finish().expect("finishes");
    out
}

#[test]
fn rgba_survives_a_round_trip() {
    let pixels: Vec<u8> = vec![
        255, 0, 0, 255, // red
        0, 255, 0, 128, // half-transparent green
        0, 0, 255, 0, // fully transparent blue
        1, 2, 3, 4, //
        5, 6, 7, 8, //
        250, 251, 252, 253,
    ];
    let png = encode(&pixels, 3, 2, 32);

    assert_eq!(decode(&png, KRKR_PNG_RGBA, -1, pixels.len()), Some(pixels));

    let info = info(&png).expect("reads the header");
    assert_eq!((info.width, info.height), (3, 2));
    assert_eq!((info.color_type, info.bit_depth, info.bpp), (6, 8, 32));
    assert_ne!(info.has_alpha, 0);
}

#[test]
fn rgb_gets_an_opaque_alpha() {
    let pixels: Vec<u8> = vec![10, 20, 30, 40, 50, 60];
    let png = encode(&pixels, 2, 1, 24);

    assert_eq!(
        decode(&png, KRKR_PNG_RGBA, -1, 8),
        Some(vec![10, 20, 30, 255, 40, 50, 60, 255])
    );

    let info = info(&png).expect("reads the header");
    assert_eq!((info.color_type, info.bpp), (2, 24));
    assert_eq!(info.has_alpha, 0);
}

#[test]
fn palette_indices_are_kept_and_expanded() {
    let mut png = Vec::new();
    let mut encoder = Encoder::new(&mut png, 3, 1);
    encoder.set_color(ColorType::Indexed);
    encoder.set_depth(BitDepth::Eight);
    encoder.set_palette(vec![255, 0, 0, 0, 255, 0, 0, 0, 255]);
    encoder.set_trns(vec![255, 128, 255]);
    let mut writer = encoder.write_header().expect("writes a header");
    writer.write_image_data(&[0, 1, 2]).expect("encodes");
    writer.finish().expect("finishes");

    assert_eq!(decode(&png, KRKR_PNG_INDEX, -1, 3), Some(vec![0, 1, 2]));

    let palette = vec![255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 255];
    assert_eq!(decode(&png, KRKR_PNG_RGBA, -1, 12), Some(palette));

    // `key` makes one index transparent.
    let keyed = vec![255, 0, 0, 255, 0, 255, 0, 0, 0, 0, 255, 255];
    assert_eq!(decode(&png, KRKR_PNG_RGBA, 1, 12), Some(keyed));

    let info = info(&png).expect("reads the header");
    assert_eq!((info.color_type, info.bpp), (3, 8));
    assert_ne!(info.has_alpha, 0);
}

#[test]
fn gray_becomes_gray_or_color() {
    let samples: Vec<u8> = vec![0, 85, 170, 255];
    let png = fixture(4, 1, ColorType::Grayscale, BitDepth::Eight, &samples);

    assert_eq!(decode(&png, KRKR_PNG_GRAY, -1, 4), Some(samples.clone()));
    assert_eq!(decode(&png, KRKR_PNG_INDEX, -1, 4), Some(samples));

    assert_eq!(
        decode(&png, KRKR_PNG_RGBA, -1, 16),
        Some(vec![
            0, 0, 0, 255, 85, 85, 85, 255, 170, 170, 170, 255, 255, 255, 255, 255
        ])
    );
    assert_eq!(info(&png).expect("reads the header").bpp, 8);
}

#[test]
fn sub_byte_gray_samples_are_scaled() {
    let png = fixture(8, 1, ColorType::Grayscale, BitDepth::One, &[0b1010_1010]);

    assert_eq!(
        decode(&png, KRKR_PNG_INDEX, -1, 8),
        Some(vec![255, 0, 255, 0, 255, 0, 255, 0])
    );
}

#[test]
fn gray_alpha_drops_the_alpha_channel() {
    let png = fixture(2, 1, ColorType::GrayscaleAlpha, BitDepth::Eight, &[40, 10, 200, 20]);

    assert_eq!(decode(&png, KRKR_PNG_GRAY, -1, 2), Some(vec![40, 200]));
    assert_eq!(
        decode(&png, KRKR_PNG_RGBA, -1, 8),
        Some(vec![40, 40, 40, 10, 200, 200, 200, 20])
    );
}

#[test]
fn color_becomes_gray_with_the_png_coefficients() {
    // Rec. 709 luminance: 0.2126 * 255 rounds to 54.
    let png = fixture(1, 1, ColorType::Rgb, BitDepth::Eight, &[255, 0, 0]);

    assert_eq!(decode(&png, KRKR_PNG_GRAY, -1, 1), Some(vec![54]));
}

#[test]
fn metainfo_chunks_are_reported() {
    let mut png = fixture(1, 1, ColorType::Grayscale, BitDepth::Eight, &[0]);

    // oFFs and vpAg, which the encoder does not write.
    let offset = [0, 0, 0, 3, 0, 0, 0, 5, 1]; // x 3, y 5, micrometer
    let vpage = [0, 0, 2, 128, 0, 0, 2, 88, 0]; // 640 x 600, pixel
    let mut chunks = chunk(b"oFFs", &offset);
    chunks.extend(chunk(b"vpAg", &vpage));
    let end = png.len() - 12; // before IEND
    png.splice(end..end, chunks);

    let info = info(&png).expect("reads the header");
    assert_ne!(info.has_offset, 0);
    assert_eq!((info.offset_x, info.offset_y, info.offset_unit), (3, 5, 1));
    assert_ne!(info.has_vpage, 0);
    assert_eq!((info.vpage_width, info.vpage_height, info.vpage_unit), (640, 600, 0));
    assert_eq!(info.has_resolution, 0);
}

fn chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in kind.iter().chain(data) {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { crc >> 1 ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    out.extend_from_slice(&(!crc).to_be_bytes());
    out
}

#[test]
fn rejects_what_is_not_a_png() {
    assert!(info(b"").is_none());
    assert!(info(b"not a png at all").is_none());
    assert_eq!(decode(b"\x89PNG", KRKR_PNG_RGBA, -1, 0), None);
}
