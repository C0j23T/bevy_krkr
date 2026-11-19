use super::{guard, input};
use image::{ColorType, DynamicImage, ImageDecoder, ImageFormat, ImageReader};
use std::{borrow::Cow, ffi::c_void, io::Cursor};

const MAX_BITMAP: usize = 256 << 20;
const HAS_ALPHA: u32 = 0x0002;
const HAS_TRANSLUCENT: u32 = 0x0004;
const REAL_DPI: u32 = 0x1000;
const REAL_PIXEL_SIZE: u32 = 0x2000;

#[repr(C)]
pub struct DrawingBitmapInfo {
    width: u32,
    height: u32,
    pixel_format: u32,
    flags: u32,
    dpi: [f64; 2],
}

fn jpeg_resolution(bytes: &[u8]) -> Option<[f64; 2]> {
    let mut bytes = bytes.get(2..)?;
    while bytes.first() == Some(&255) {
        bytes = &bytes[1..];
        if bytes.first() == Some(&255) {
            continue;
        }
        let tag = *bytes.first()?;
        if matches!(tag, 0xda | 0xd9) {
            break;
        }
        let size = u16::from_be_bytes(bytes.get(1..3)?.try_into().ok()?) as usize;
        let data = bytes.get(3..1 + size)?;
        if tag == 0xe0 && data.starts_with(b"JFIF\0") {
            let scale = match *data.get(7)? {
                1 => 1.,
                2 => 2.54,
                _ => return None,
            };
            let x = u16::from_be_bytes(data.get(8..10)?.try_into().ok()?);
            let y = u16::from_be_bytes(data.get(10..12)?.try_into().ok()?);
            return Some([f64::from(x) * scale, f64::from(y) * scale]);
        }
        bytes = bytes.get(1 + size..)?;
    }
    None
}

fn tiff_resolution(bytes: &[u8]) -> Option<[f64; 2]> {
    use tiff::{
        decoder::{Decoder, ifd::Value},
        tags::Tag,
    };
    let mut d = Decoder::new(Cursor::new(bytes)).ok()?;
    let scale = match d.get_tag_unsigned::<u16>(Tag::ResolutionUnit).unwrap_or(2) {
        2 => 1.,
        3 => 2.54,
        _ => return None,
    };
    let mut resolution = |tag| match d.get_tag(tag).ok()? {
        Value::Rational(n, d) if d > 0 => Some(scale * f64::from(n) / f64::from(d)),
        _ => None,
    };
    Some([resolution(Tag::XResolution)?, resolution(Tag::YResolution)?])
}

fn indexed_format(bits: u8) -> u32 {
    match bits {
        1 => 0x30101,
        2 | 4 => 0x30402,
        _ => 0x30803,
    }
}

fn bmp_metadata(bytes: &[u8], default_format: u32) -> Option<(u32, Option<[f64; 2]>)> {
    let u16_at = |at| Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?));
    let u32_at = |at| Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?));
    let header = u32_at(14)?;
    let bpp = u16_at(if header == 12 { 24 } else { 28 })?;
    let format = match bpp {
        1..=8 => indexed_format(bpp as u8),
        16 if header >= 40 && u32_at(30)? == 3 && u32_at(58)? == 0x07e0 => 0x21006,
        16 => 0x21005,
        32 if default_format == 0x21808 => 0x22009,
        _ => default_format,
    };
    let dpi = if header >= 40 {
        Some([38, 42].map(|at| u32_at(at).map_or(0., |v| f64::from(v as i32) * 0.0254)))
    } else {
        None
    };
    Some((format, dpi))
}

fn bmp_legacy_alpha(bytes: &[u8], rgba: &mut image::RgbaImage) -> Option<()> {
    let u32_at = |at| Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?));
    if u32_at(14)? != 40 || u32_at(30)? != 0 {
        return None;
    }
    let offset = u32_at(10)? as usize;
    let height = u32_at(22)? as i32;
    let row_bytes = rgba.width() as usize * 4;
    let pixels = bytes
        .get(offset..)?
        .get(..row_bytes * rgba.height() as usize)?;
    let mut rows = pixels.chunks_exact(row_bytes);
    for row in rgba.rows_mut() {
        let source = if height < 0 {
            rows.next()?
        } else {
            rows.next_back()?
        };
        for (pixel, raw) in row.zip(source.chunks_exact(4)) {
            pixel[3] = raw[3];
        }
    }
    Some(())
}

fn decode_drawing_bitmap(bytes: &[u8]) -> Result<(DrawingBitmapInfo, Vec<u8>), String> {
    let reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let format = reader.format();
    let decoder = reader.into_decoder().map_err(|e| e.to_string())?;
    let (w, h) = decoder.dimensions();
    if w == 0
        || h == 0
        || u64::from(w) * u64::from(h) > (MAX_BITMAP / 4) as u64
        || decoder.total_bytes() > MAX_BITMAP as u64
    {
        return Err("drawing bitmap exceeds image limit".into());
    }
    let color = decoder.color_type();
    let mut pixel_format = match color {
        ColorType::L8 => 0x30803,
        ColorType::L16 => 0x101004,
        ColorType::Rgb8 => 0x21808,
        ColorType::Rgb16 => 0x10300c,
        ColorType::Rgba16 => 0x34400d,
        _ => 0x26200a,
    };
    let mut has_alpha = color.has_alpha();
    let mut color_space = if color.has_color() { 0x10 } else { 0x40 };
    let resolution = match format {
        Some(ImageFormat::Png) => {
            let png = crate::png::Png::parse(bytes).ok_or("invalid PNG header")?;
            if png.color_type == 3 || (png.color_type == 0 && png.bit_depth <= 8) {
                pixel_format = indexed_format(png.bit_depth);
            }
            has_alpha |= png.has_alpha();
            png.resolution
                .filter(|r| r.2 == 1)
                .map(|(x, y, _)| [f64::from(x) * 0.0254, f64::from(y) * 0.0254])
        }
        Some(ImageFormat::Bmp) => {
            let (format, dpi) = bmp_metadata(bytes, pixel_format).ok_or("invalid BMP header")?;
            pixel_format = format;
            dpi
        }
        Some(ImageFormat::Gif) => {
            pixel_format = 0x30803;
            None
        }
        Some(ImageFormat::Tiff) => tiff_resolution(bytes),
        Some(ImageFormat::Jpeg) => {
            if color.has_color() {
                color_space = 0x80;
            }
            jpeg_resolution(bytes)
        }
        _ => None,
    }
    .filter(|dpi| dpi.iter().all(|v| v.is_finite() && *v > 0.));
    let mut rgba = DynamicImage::from_decoder(decoder)
        .map_err(|e| e.to_string())?
        .into_rgba8();
    // Kirikiri stores alpha in the unused byte of its 32-bit BI_RGB screenshots.
    if format == Some(ImageFormat::Bmp)
        && pixel_format == 0x22009
        && bmp_legacy_alpha(bytes, &mut rgba).is_some()
    {
        pixel_format = 0x26200a;
        has_alpha = true;
    }
    let mut flags = REAL_PIXEL_SIZE | color_space;
    if resolution.is_some() {
        flags |= REAL_DPI;
    }
    if has_alpha {
        if format != Some(ImageFormat::Gif) {
            flags |= HAS_ALPHA;
        }
        for pixel in rgba.pixels() {
            match pixel[3] {
                255 => (),
                0 => flags |= HAS_ALPHA,
                _ => {
                    flags |= HAS_ALPHA | HAS_TRANSLUCENT;
                    break;
                }
            }
        }
    }
    Ok((
        DrawingBitmapInfo {
            width: w,
            height: h,
            pixel_format,
            flags,
            dpi: resolution.unwrap_or([96.; 2]),
        },
        rgba.into_raw(),
    ))
}

/// # Safety
/// Input is readable for length bytes. All output pointers are writable.
/// Release the returned RGBA buffer with krkr_png_free.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_drawing_bitmap_decode(
    data: *const c_void,
    length: usize,
    info: *mut DrawingBitmapInfo,
    rgba: *mut *mut c_void,
    size: *mut usize,
) -> i32 {
    guard(-1, || {
        let (Some(info), Some(out), Some(size)) = (info.as_mut(), rgba.as_mut(), size.as_mut())
        else {
            return -1;
        };
        if length > MAX_BITMAP {
            return -1;
        }
        let Ok((metadata, bytes)) = decode_drawing_bitmap(input(data, length)) else {
            return -1;
        };
        (*info, *size) = (metadata, bytes.len());
        *out = Box::into_raw(bytes.into_boxed_slice()).cast::<u8>().cast();
        0
    })
}

#[repr(C)]
pub struct DrawingEncoder {
    format: u32,
    depth: i32,
    compression: i32,
    scan: i32,
    version: i32,
    render: i32,
    quality: i32,
}

fn encode(
    bytes: &[u8],
    w: u32,
    h: u32,
    options: &DrawingEncoder,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut out = Vec::new();
    match options.format {
        1 => {
            let mut rgba = bytes.to_vec();
            let mut frame =
                gif::Frame::from_rgba_speed(w.try_into()?, h.try_into()?, &mut rgba, 10);
            if options.scan == 7 {
                let mut indices = Vec::with_capacity(frame.buffer.len());
                for (start, step) in [(0, 8), (4, 8), (2, 4), (1, 2)] {
                    for y in (start..h as usize).step_by(step) {
                        let from = y * w as usize;
                        indices.extend_from_slice(&frame.buffer[from..from + w as usize]);
                    }
                }
                frame.buffer = Cow::Owned(indices);
                frame.interlaced = true;
            }
            let mut encoder = gif::Encoder::new(&mut out, frame.width, frame.height, &[])?;
            encoder.write_frame(&frame)?;
            encoder.into_inner()?;
            if options.version == 9 {
                // GIF87a has no graphic control extension. Drop its transparency along with it.
                let palette_len = if out[10] & 0x80 != 0 {
                    3 << ((out[10] & 7) + 1)
                } else {
                    0
                };
                let start = 13 + palette_len;
                if out.get(start..start + 3) == Some(&[0x21, 0xf9, 4]) {
                    out.drain(start..start + 8);
                }
                out[..6].copy_from_slice(b"GIF87a");
            }
        }
        2 => {
            use tiff::encoder::{Compression, TiffEncoder, colortype};
            let compression = match options.compression {
                -1 | 2 => Compression::Lzw,
                5 => Compression::Packbits,
                6 => Compression::Uncompressed,
                _ => return Err("unsupported TIFF compression".into()),
            };
            let mut encoder =
                TiffEncoder::new(Cursor::new(&mut out))?.with_compression(compression);
            match options.depth {
                24 => {
                    let rgb: Vec<_> = bytes
                        .chunks_exact(4)
                        .flat_map(|pixel| pixel[..3].iter().copied())
                        .collect();
                    encoder.write_image::<colortype::RGB8>(w, h, &rgb)?;
                }
                32 => encoder.write_image::<colortype::RGBA8>(w, h, bytes)?,
                _ => return Err("unsupported TIFF depth".into()),
            }
        }
        3 => {
            let mut encoder =
                jpeg_encoder::Encoder::new(&mut out, options.quality.clamp(1, 100) as u8);
            encoder.set_progressive(options.render == 11);
            encoder.encode(
                bytes,
                w.try_into()?,
                h.try_into()?,
                jpeg_encoder::ColorType::Rgba,
            )?;
        }
        _ => return Err("unsupported drawing image format".into()),
    }
    Ok(out)
}

/// # Safety
/// Pixels are RGBA and readable for length bytes. Options and outputs must be valid.
/// Release the encoded bytes with krkr_png_free.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_drawing_bitmap_encode(
    pixels: *const c_void,
    length: usize,
    w: u32,
    h: u32,
    options: *const DrawingEncoder,
    out: *mut *mut c_void,
    size: *mut usize,
) -> i32 {
    guard(-1, || {
        let (Some(options), Some(out), Some(size)) =
            (options.as_ref(), out.as_mut(), size.as_mut())
        else {
            return -1;
        };
        if pixels.is_null()
            || length > MAX_BITMAP
            || w == 0
            || h == 0
            || u64::from(w) * u64::from(h) != (length / 4) as u64
            || length % 4 != 0
        {
            return -1;
        }
        let Ok(data) = encode(input(pixels, length), w, h, options) else {
            return -1;
        };
        *size = data.len();
        *out = Box::into_raw(data.into_boxed_slice()).cast::<u8>().cast();
        0
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_preserves_indexed_transparency_and_physical_resolution() {
        let mut encoded = Vec::new();
        let mut encoder = png::Encoder::new(&mut encoded, 2, 1);
        encoder.set_color(png::ColorType::Indexed);
        encoder.set_depth(png::BitDepth::Four);
        encoder.set_palette(vec![255, 0, 0, 0, 0, 255]);
        encoder.set_trns(vec![128, 255]);
        encoder.set_pixel_dims(Some(png::PixelDimensions {
            xppu: 10_000,
            yppu: 5_000,
            unit: png::Unit::Meter,
        }));
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&[0x01]).unwrap();
        writer.finish().unwrap();

        let (info, rgba) = decode_drawing_bitmap(&encoded).unwrap();
        assert_eq!(
            (info.width, info.height, info.pixel_format),
            (2, 1, 0x30402)
        );
        assert_eq!(
            info.flags & (REAL_DPI | HAS_ALPHA | HAS_TRANSLUCENT),
            REAL_DPI | HAS_ALPHA | HAS_TRANSLUCENT
        );
        assert_eq!(info.dpi, [254., 127.]);
        assert_eq!(rgba, [255, 0, 0, 128, 0, 0, 255, 255]);
    }

    #[test]
    fn bmp_accepts_top_down_and_bottom_up_screenshots_with_alpha() {
        for height in [-2i32, 2] {
            let mut encoded = vec![0; 54];
            encoded[..2].copy_from_slice(b"BM");
            for (at, value) in [
                (2, 62),
                (10, 54),
                (14, 40),
                (18, 1),
                (22, height),
                (26, 1 | (32 << 16)),
                (38, 10_000),
                (42, 5_000),
            ] {
                encoded[at..at + 4].copy_from_slice(&value.to_le_bytes());
            }
            let rows = [[0, 0, 255, 128], [255, 0, 0, 0]];
            for row in if height > 0 { [rows[1], rows[0]] } else { rows } {
                encoded.extend_from_slice(&row);
            }
            let (info, rgba) = decode_drawing_bitmap(&encoded).unwrap();
            assert_eq!(
                (info.width, info.height, info.pixel_format),
                (1, 2, 0x26200a)
            );
            assert_eq!(info.dpi, [254., 127.]);
            assert_eq!(rgba, [255, 0, 0, 128, 0, 0, 255, 0]);
        }
    }

    #[test]
    fn tiff_and_jpeg_read_the_stored_physical_resolution() {
        use tiff::{
            encoder::{Rational, TiffEncoder, colortype},
            tags::ResolutionUnit,
        };

        let mut encoded = Vec::new();
        let mut tiff = TiffEncoder::new(Cursor::new(&mut encoded)).unwrap();
        let mut image = tiff.new_image::<colortype::RGBA8>(1, 1).unwrap();
        image.resolution_unit(ResolutionUnit::Centimeter);
        image.x_resolution(Rational { n: 100, d: 1 });
        image.y_resolution(Rational { n: 50, d: 1 });
        image.write_data(&[90, 20, 3, 80]).unwrap();
        let (info, rgba) = decode_drawing_bitmap(&encoded).unwrap();
        assert_eq!(info.dpi, [254., 127.]);
        assert_eq!(rgba, [90, 20, 3, 80]);

        encoded.clear();
        let mut jpeg = jpeg_encoder::Encoder::new(&mut encoded, 100);
        jpeg.set_density(jpeg_encoder::PixelDensity {
            density: (144, 72),
            unit: jpeg_encoder::PixelDensityUnit::Inches,
        });
        jpeg.encode(&[90, 20, 3], 1, 1, jpeg_encoder::ColorType::Rgb)
            .unwrap();
        let (info, rgba) = decode_drawing_bitmap(&encoded).unwrap();
        assert_eq!(info.dpi, [144., 72.]);
        assert_eq!(info.flags & (REAL_DPI | HAS_ALPHA), REAL_DPI);
        assert!(
            rgba.iter()
                .zip([90, 20, 3, 255])
                .all(|(&a, b)| a.abs_diff(b) <= 2)
        );
    }
}
