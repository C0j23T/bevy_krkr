//! The JPEG codec through the C ABI, the way `visual/LoadJPEG.cpp` drives it:
//! the header call, the two row layouts the engine bitmap asks for, and the
//! encoder. Fixtures come from `image`'s encoder, so this tests the seam
//! rather than the DCT.

use jpeg_encoder::{ColorType, Encoder, SamplingFactor};
use krkr_shim::ffi::{krkr_jpeg_decode, krkr_jpeg_encode, krkr_jpeg_free, krkr_jpeg_info};

fn encode(pixels: &[u8], width: u32, height: u32, color: ColorType) -> Vec<u8> {
    let mut jpeg = Vec::new();
    let mut encoder = Encoder::new(&mut jpeg, 100);
    encoder.set_sampling_factor(SamplingFactor::R_4_4_4);
    encoder
        .encode(pixels, width as u16, height as u16, color)
        .expect("encodes");
    jpeg
}

fn info(jpeg: &[u8]) -> Option<(u32, u32)> {
    let (mut width, mut height) = (0, 0);
    let status = unsafe { krkr_jpeg_info(jpeg.as_ptr().cast(), jpeg.len(), &mut width, &mut height) };
    (status == 0).then_some((width, height))
}

fn decode(jpeg: &[u8], gray: bool, len: usize) -> Option<Vec<u8>> {
    let mut out = vec![0u8; len];
    let written = unsafe {
        krkr_jpeg_decode(
            jpeg.as_ptr().cast(),
            jpeg.len(),
            gray as i32,
            out.as_mut_ptr().cast(),
            out.len(),
        )
    };
    (written == len).then_some(out)
}

fn encode_rgba(pixels: &[u8], width: u32, height: u32) -> Vec<u8> {
    let mut jpeg = std::ptr::null_mut();
    let mut len = 0;
    let status = unsafe {
        krkr_jpeg_encode(
            pixels.as_ptr().cast(),
            pixels.len(),
            width,
            height,
            100,
            2,
            &mut jpeg,
            &mut len,
        )
    };
    assert_eq!(status, 0, "encodes");
    let out = unsafe { std::slice::from_raw_parts(jpeg as *const u8, len) }.to_vec();
    unsafe { krkr_jpeg_free(jpeg, len) };
    out
}

fn near(actual: &[u8], expected: &[u8], what: &str) {
    within(actual, expected, 3, what);
}

fn within(actual: &[u8], expected: &[u8], tolerance: u8, what: &str) {
    assert_eq!(actual.len(), expected.len(), "{what}: length");
    for (index, (&a, &e)) in actual.iter().zip(expected).enumerate() {
        assert!(
            a.abs_diff(e) <= tolerance,
            "{what}: byte {index} is {a}, expected {e}"
        );
    }
}

#[test]
fn rgb_decodes_to_opaque_rgba() {
    let jpeg = encode(&[255, 0, 0, 0, 0, 255], 2, 1, ColorType::Rgb);

    assert_eq!(info(&jpeg), Some((2, 1)));
    near(
        &decode(&jpeg, false, 8).expect("decodes"),
        &[255, 0, 0, 255, 0, 0, 255, 255],
        "rgba",
    );
}

#[test]
fn a_gray_source_is_expanded_on_request() {
    let jpeg = encode(&[0, 85, 170, 255], 4, 1, ColorType::Luma);

    assert_eq!(info(&jpeg), Some((4, 1)));
    near(&decode(&jpeg, true, 4).expect("decodes"), &[0, 85, 170, 255], "gray");
    near(
        &decode(&jpeg, false, 16).expect("decodes"),
        &[0, 0, 0, 255, 85, 85, 85, 255, 170, 170, 170, 255, 255, 255, 255, 255],
        "gray as rgba",
    );
}

#[test]
fn color_becomes_gray_on_request() {
    let jpeg = encode(&[255, 0, 0, 0, 0, 255], 2, 1, ColorType::Rgb);

    let gray = decode(&jpeg, true, 2).expect("decodes");
    assert!(gray[0] > 70 && gray[0] < 90, "red luma is {}", gray[0]);
    assert!(gray[1] > 20 && gray[1] < 40, "blue luma is {}", gray[1]);
}

#[test]
fn a_round_trip_keeps_the_colors() {
    let pixels: Vec<u8> = vec![
        255, 0, 0, 12, // the fourth byte is ignored
        0, 255, 0, 34, //
        0, 0, 255, 56, //
        128, 128, 128, 78,
    ];
    let jpeg = encode_rgba(&pixels, 2, 2);

    assert_eq!(info(&jpeg), Some((2, 2)));
    let expected: Vec<u8> = pixels
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| [p[0], p[1], p[2], 255])
        .collect();
    within(&decode(&jpeg, false, 16).expect("decodes"), &expected, 16, "round trip");
}

#[test]
fn rejects_what_is_not_a_jpeg() {
    assert_eq!(info(b""), None);
    assert_eq!(info(b"not a jpeg at all"), None);
    assert_eq!(decode(b"\xff\xd8", false, 16), None);
}
