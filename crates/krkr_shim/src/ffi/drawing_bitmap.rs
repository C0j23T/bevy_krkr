use super::{guard, input};
use image::{DynamicImage, ImageDecoder, ImageReader};
use std::{borrow::Cow, ffi::c_void, io::Cursor};

const MAX_BITMAP: usize = 256 << 20;

pub fn decode_drawing_bitmap(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    let reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let decoder = reader.into_decoder().map_err(|e| e.to_string())?;
    let (w, h) = decoder.dimensions();
    if w == 0
        || h == 0
        || u64::from(w) * u64::from(h) > (MAX_BITMAP / 4) as u64
        || decoder.total_bytes() > MAX_BITMAP as u64
    {
        return Err("drawing bitmap exceeds image limit".into());
    }
    let rgba = DynamicImage::from_decoder(decoder)
        .map_err(|e| e.to_string())?
        .into_rgba8();
    Ok((w, h, rgba.into_raw()))
}

/// # Safety
/// Input is readable for length bytes. All output pointers are writable.
/// Release the returned RGBA buffer with krkr_png_free.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_drawing_bitmap_decode(
    data: *const c_void,
    length: usize,
    width: *mut u32,
    height: *mut u32,
    rgba: *mut *mut c_void,
    size: *mut usize,
) -> i32 {
    guard(-1, || {
        let (Some(w), Some(h), Some(out), Some(size)) = (
            width.as_mut(),
            height.as_mut(),
            rgba.as_mut(),
            size.as_mut(),
        ) else {
            return -1;
        };
        if length > MAX_BITMAP {
            return -1;
        }
        let Ok((x, y, bytes)) = decode_drawing_bitmap(input(data, length)) else {
            return -1;
        };
        (*w, *h, *size) = (x, y, bytes.len());
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
