//! JPEG for `visual/LoadJPEG.cpp`: the header, decoding into the two row
//! layouts an engine bitmap uses, and encoding.

use std::io::Cursor;

use jpeg_encoder::{ColorType, Encoder, SamplingFactor};
use zune_jpeg::zune_core::colorspace::ColorSpace;
use zune_jpeg::zune_core::options::DecoderOptions;
use zune_jpeg::JpegDecoder;

/// `(width, height, source components)`, without decoding the image.
fn header(bytes: &[u8]) -> Option<(u32, u32, u8)> {
    let mut decoder = JpegDecoder::new(Cursor::new(bytes));
    decoder.decode_headers().ok()?;
    let info = decoder.info()?;
    Some((info.width as u32, info.height as u32, info.components))
}

/// Pixel size, as the engine needs it before decoding.
pub fn size(bytes: &[u8]) -> Option<(u32, u32)> {
    let (width, height, _) = header(bytes)?;
    Some((width, height))
}

/// `(width, height, pixels)`: four bytes per pixel, or one when `gray`.
pub fn decode(bytes: &[u8], gray: bool) -> Option<(u32, u32, Vec<u8>)> {
    let (width, height, components) = header(bytes)?;
    // A gray source has no chroma to convert, so it is decoded as it is and
    // expanded below: asking a decoder for a colour space the file does not
    // carry is not something they all agree on.
    let luma = gray || components == 1;
    let colorspace = if luma { ColorSpace::Luma } else { ColorSpace::RGBA };
    let options = DecoderOptions::default().jpeg_set_out_colorspace(colorspace);

    let mut decoder = JpegDecoder::new_with_options(Cursor::new(bytes), options);
    let pixels = decoder.decode().ok()?;
    let (width, height) = (width as usize, height as usize);
    if pixels.len() != width * height * if luma { 1 } else { 4 } {
        return None;
    }

    let pixels = if gray || !luma {
        pixels
    } else {
        pixels.iter().flat_map(|&g| [g, g, g, 255]).collect()
    };
    Some((width as u32, height as u32, pixels))
}

/// Encodes four byte pixels, ignoring the fourth. `subsampling` is one of the
/// `KRKR_JPEG_*` values.
pub fn encode(
    pixels: &[u8],
    width: u32,
    height: u32,
    quality: u8,
    subsampling: i32,
) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut encoder = Encoder::new(&mut out, quality);
    encoder.set_sampling_factor(match subsampling {
        1 => SamplingFactor::R_4_2_2,
        2 => SamplingFactor::R_4_4_4,
        _ => SamplingFactor::R_4_2_0,
    });
    encoder.set_progressive(true);
    encoder.set_optimized_huffman_tables(true);
    encoder
        .encode(pixels, width.try_into().ok()?, height.try_into().ok()?, ColorType::Rgba)
        .ok()?;
    Some(out)
}
