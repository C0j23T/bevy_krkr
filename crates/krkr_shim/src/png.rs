//! PNG for `visual/LoadPNG.cpp`: the header and metainfo chunks, decoding into
//! the row layouts an engine bitmap uses, and encoding.

use std::io::Cursor;

use png::{BitDepth, ColorType, Decoder, Encoder, Transformations};

const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

const GRAY: u8 = 0;
const RGB: u8 = 2;
const PALETTE: u8 = 3;
const GRAY_ALPHA: u8 = 4;
const RGBA: u8 = 6;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Rgba,
    Gray,
    Index,
}

/// IHDR and the chunks the engine reports as metainfo.
#[derive(Default)]
pub struct Png {
    pub width: u32,
    pub height: u32,
    pub bit_depth: u8,
    pub color_type: u8,
    pub offset: Option<(i32, i32, u8)>,
    pub resolution: Option<(u32, u32, u8)>,
    pub vpage: Option<(u32, u32, u8)>,
    palette: Vec<u8>,
    trns: Vec<u8>,
}

impl Png {
    /// Reads the chunk headers only; the image data is not touched.
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        if bytes.get(..8)? != SIGNATURE {
            return None;
        }
        let mut png = Png::default();
        let mut rest = bytes.get(8..)?;
        loop {
            let len = be32(rest)? as usize;
            let kind = rest.get(4..8)?;
            let data = rest.get(8..8usize.checked_add(len)?)?;
            rest = rest.get(12usize.checked_add(len)?..)?;
            match kind {
                b"IHDR" => {
                    png.width = be32(data)?;
                    png.height = be32(data.get(4..)?)?;
                    png.bit_depth = *data.get(8)?;
                    png.color_type = *data.get(9)?;
                }
                b"PLTE" => png.palette = data.to_vec(),
                b"tRNS" => png.trns = data.to_vec(),
                b"oFFs" => {
                    let (x, y) = (be32(data)? as i32, be32(data.get(4..)?)? as i32);
                    png.offset = Some((x, y, *data.get(8)?));
                }
                b"pHYs" => {
                    png.resolution = Some((be32(data)?, be32(data.get(4..)?)?, *data.get(8)?))
                }
                b"vpAg" => png.vpage = Some((be32(data)?, be32(data.get(4..)?)?, *data.get(8)?)),
                b"IEND" => {
                    return (png.width > 0 && png.height > 0).then_some(png);
                }
                _ => {}
            }
        }
    }

    /// Bits per pixel of the source image, as `Bitmap.loadHeader` reports it.
    pub fn bpp(&self) -> i32 {
        let channels = match self.color_type {
            GRAY | PALETTE => 1,
            GRAY_ALPHA => 2,
            RGB => 3,
            RGBA => 4,
            _ => return 0,
        };
        self.bit_depth as i32 * channels
    }

    /// Whether a full-color load keeps an alpha channel: the image stores one,
    /// or it is paletted with transparency.
    pub fn has_alpha(&self) -> bool {
        matches!(self.color_type, GRAY_ALPHA | RGBA)
            || (self.color_type == PALETTE && !self.trns.is_empty())
    }
}

pub fn decode(bytes: &[u8], target: Target, key: i32) -> Option<Vec<u8>> {
    let png = Png::parse(bytes)?;
    let (width, height) = (png.width as usize, png.height as usize);

    if png.color_type == PALETTE {
        let index = indices(bytes, &png, width, height)?;
        return match target {
            Target::Index => Some(index),
            Target::Rgba => Some(palette_rgba(&index, &png, key)),
            Target::Gray => Some(luma(&palette_rgba(&index, &png, key))),
        };
    }

    if target == Target::Index {
        // A province image keeps the sample values; only gray has them.
        return match png.color_type {
            GRAY => Some(scale(&indices(bytes, &png, width, height)?, png.bit_depth)),
            _ => None,
        };
    }

    let (color, data) = read(bytes, Transformations::EXPAND | Transformations::STRIP_16)?;
    Some(match target {
        Target::Rgba => expand_rgba(&color, &data, !png.has_alpha()),
        _ => expand_gray(&color, &data),
    })
}

/// Encodes 24 or 32 bit pixels, row by row with no padding.
pub fn encode(pixels: &[u8], width: u32, height: u32, bpp: u32) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut encoder = Encoder::new(&mut out, width, height);
    encoder.set_color(if bpp == 32 { ColorType::Rgba } else { ColorType::Rgb });
    encoder.set_depth(BitDepth::Eight);
    let mut writer = encoder.write_header().ok()?;
    writer.write_image_data(pixels).ok()?;
    writer.finish().ok()?;
    Some(out)
}

/// Decodes to whatever `transform` leaves: for [`Transformations::IDENTITY`]
/// the samples stay packed in the source's own bit depth.
fn read(bytes: &[u8], transform: Transformations) -> Option<(ColorType, Vec<u8>)> {
    let mut decoder = Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(transform);
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let frame = reader.next_frame(&mut buf).ok()?;
    buf.truncate(frame.buffer_size());
    let (color, _) = reader.output_color_type();
    Some((color, buf))
}

/// One byte per sample, so 1, 2 and 4 bit samples are unpacked in place.
fn indices(bytes: &[u8], png: &Png, width: usize, height: usize) -> Option<Vec<u8>> {
    let (_, data) = read(bytes, Transformations::IDENTITY)?;
    let depth = png.bit_depth;
    if depth == 8 {
        return (data.len() >= width * height).then(|| data[..width * height].to_vec());
    }
    if !matches!(depth, 1 | 2 | 4) {
        return None;
    }

    let stride = (width * depth as usize).div_ceil(8);
    let mask = (1u16 << depth) - 1;
    let mut out = Vec::with_capacity(width * height);
    for y in 0..height {
        let row = data.get(y * stride..(y + 1) * stride)?;
        for x in 0..width {
            let bit = x * depth as usize;
            out.push(((row[bit / 8] >> (8 - depth as usize - bit % 8)) as u16 & mask) as u8);
        }
    }
    Some(out)
}

fn palette_rgba(index: &[u8], png: &Png, key: i32) -> Vec<u8> {
    let mut out = Vec::with_capacity(index.len() * 4);
    for &i in index {
        let entry = &png.palette.get(i as usize * 3..i as usize * 3 + 3).unwrap_or(&[0, 0, 0]);
        let alpha = if i as i32 == key {
            0
        } else {
            png.trns.get(i as usize).copied().unwrap_or(255)
        };
        out.extend_from_slice(entry);
        out.push(alpha);
    }
    out
}

fn expand_rgba(color: &ColorType, data: &[u8], opaque: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() / channels(*color) * 4);
    match color {
        ColorType::Grayscale => data.iter().for_each(|&g| out.extend_from_slice(&[g, g, g, 255])),
        ColorType::GrayscaleAlpha => data
            .as_chunks::<2>()
            .0
            .iter()
            .for_each(|p| out.extend_from_slice(&[p[0], p[0], p[0], p[1]])),
        ColorType::Rgb => data
            .as_chunks::<3>()
            .0
            .iter()
            .for_each(|p| out.extend_from_slice(&[p[0], p[1], p[2], 255])),
        _ => out.extend_from_slice(data),
    }
    if opaque {
        out.iter_mut().skip(3).step_by(4).for_each(|alpha| *alpha = 255);
    }
    out
}

fn expand_gray(color: &ColorType, data: &[u8]) -> Vec<u8> {
    match color {
        ColorType::Grayscale => data.to_vec(),
        ColorType::GrayscaleAlpha => data.as_chunks::<2>().0.iter().map(|p| p[0]).collect(),
        ColorType::Rgb => data
            .as_chunks::<3>()
            .0
            .iter()
            .map(|p| luma_of(p[0], p[1], p[2]))
            .collect(),
        _ => data
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| luma_of(p[0], p[1], p[2]))
            .collect(),
    }
}

fn luma(rgba: &[u8]) -> Vec<u8> {
    rgba.as_chunks::<4>()
        .0
        .iter()
        .map(|p| luma_of(p[0], p[1], p[2]))
        .collect()
}

/// The PNG specification's RGB to gray coefficients.
fn luma_of(r: u8, g: u8, b: u8) -> u8 {
    ((r as u32 * 6968 + g as u32 * 23434 + b as u32 * 2360) >> 15) as u8
}

/// Scales samples of less than 8 bits to the full range.
fn scale(samples: &[u8], depth: u8) -> Vec<u8> {
    if depth >= 8 {
        return samples.to_vec();
    }
    let max = (1u16 << depth) - 1;
    samples.iter().map(|&v| (v as u16 * 255 / max) as u8).collect()
}

fn channels(color: ColorType) -> usize {
    match color {
        ColorType::Grayscale => 1,
        ColorType::GrayscaleAlpha => 2,
        ColorType::Rgb => 3,
        _ => 4,
    }
}

fn be32(bytes: &[u8]) -> Option<u32> {
    Some(u32::from_be_bytes(bytes.get(..4)?.try_into().ok()?))
}
