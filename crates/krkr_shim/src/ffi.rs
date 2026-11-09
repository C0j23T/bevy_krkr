#![allow(unsafe_op_in_unsafe_fn)]

//! The C ABI of this crate
//!
//! # Rules for this layer
//!
//! - Validate everything that crosses the boundary; C++ passes null and zero
//!   lengths for empty inputs.
//! - Never unwind into C++: the engine is compiled without the tables a Rust
//!   panic needs, so a panic crossing back would abort the process. Entry
//!   points that can panic catch it and report failure.
//! - Report errors as a sentinel (`0` for the codecs, `SIZE_MAX` for the
//!   `usize` returns), because C has no way to receive a `Result`.

use std::ffi::{c_char, c_uint, c_void};
use std::io::{self, Read, Seek, SeekFrom};
use std::panic::{AssertUnwindSafe, catch_unwind};

use crate::archives::{Archive, Kind};
use crate::xml;

// ─── pointer plumbing ──────────────────────────────────────────────────────

/// Borrows a C buffer as a slice. A null pointer or a zero length is the empty
/// slice: the engine passes `(nullptr, 0)` for empty inputs.
///
/// # Safety
/// `ptr` must point to `len` readable bytes, or be null when `len == 0`.
unsafe fn input<'a>(ptr: *const c_void, len: usize) -> &'a [u8] {
    if ptr.is_null() || len == 0 {
        &[]
    } else {
        std::slice::from_raw_parts(ptr as *const u8, len)
    }
}

/// Borrows a C buffer as a mutable slice, with the same empty-input rule as
/// [`input`].
///
/// # Safety
/// `ptr` must point to `len` writable bytes, or be null when `len == 0`.
unsafe fn output<'a>(ptr: *mut c_void, len: usize) -> &'a mut [u8] {
    if ptr.is_null() || len == 0 {
        // An empty slice still needs a non-null, aligned pointer.
        std::slice::from_raw_parts_mut(std::ptr::NonNull::dangling().as_ptr(), 0)
    } else {
        std::slice::from_raw_parts_mut(ptr as *mut u8, len)
    }
}

/// Runs `body`, turning a panic into `fallback`.
fn guard<T>(fallback: T, body: impl FnOnce() -> T) -> T {
    catch_unwind(AssertUnwindSafe(body)).unwrap_or(fallback)
}

/// Error value of the `usize`-returning entry points.
const FAILED: usize = usize::MAX;

// ─── xxHash ────────────────────────────────────────────────────────────────
//
// `visual/RenderManager.cpp` deduplicates identical pixel rows of a texture
// with `XXH32`, `environ/bevy/BevyWindowLayer.cpp` detects whether a frame
// changed since the last one with `XXH3_64bits_withSeed`.
//
// These have no core counterpart below them, unlike the archive entry points:
// the whole function is one call into `xxhash-rust`, and the result is stored
// nowhere, so there is nothing to keep once C++ stops asking. That is also why
// a Rust-only future can drop this section without touching anything else.

/// `XXH32_hash_t XXH32(const void*, size_t, XXH32_hash_t)`.
///
/// # Safety
/// `input_ptr` must point to `length` readable bytes (or be null if `length` is 0).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XXH32(input_ptr: *const c_void, length: usize, seed: u32) -> u32 {
    xxhash_rust::xxh32::xxh32(input(input_ptr, length), seed)
}

/// `XXH64_hash_t XXH3_64bits_withSeed(const void*, size_t, XXH64_hash_t)`.
///
/// # Safety
/// `input_ptr` must point to `length` readable bytes (or be null if `length` is 0).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XXH3_64bits_withSeed(
    input_ptr: *const c_void,
    length: usize,
    seed: u64,
) -> u64 {
    xxhash_rust::xxh3::xxh3_64_with_seed(input(input_ptr, length), seed)
}

// ─── LZ4 (block format, no frame header) ───────────────────────────────────
//
// `visual/RenderManager.cpp` caches static textures as one compressed block per
// band of rows. The bytes are in memory only — the cache is no game data — but
// the block format is the standard one, so nothing else depends on the encoder.

/// `int LZ4_compress_default(const char*, char*, int, int)`: bytes written to
/// `dst`, or 0 when `dst_capacity` is too small.
///
/// # Safety
/// `src` must hold `src_size` bytes, `dst` must hold `dst_capacity` bytes; the
/// buffers must not overlap.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn LZ4_compress_default(
    src: *const c_void,
    dst: *mut c_void,
    src_size: i32,
    dst_capacity: i32,
) -> i32 {
    if src_size < 0 || dst_capacity < 0 {
        return 0;
    }
    let src = input(src, src_size as usize);
    let dst = output(dst, dst_capacity as usize);
    match lz4_flex::block::compress_into(src, dst) {
        Ok(written) => written as i32,
        // Upstream returns 0 for "did not fit"; the caller treats it as a
        // failure of the whole block.
        Err(_) => 0,
    }
}

/// Decompresses one LZ4 block into `dst`, which must be exactly the original
/// size. Returns the number of bytes written, or [`usize::MAX`] when the block
/// is malformed or does not produce exactly `dst_len` bytes.
///
/// This replaces `LZ4_decompress_fast(src, dst, originalSize)`, which upstream
/// liblz4 can implement because its decoder reads the input without a length.
/// The Rust decoder takes a slice, so the compressed length — already known at
/// both call sites — is passed in. That also makes the call sites' checks
/// meaningful: a block that decodes to a different size is an error rather than
/// silent memory past the end.
///
/// # Safety
/// `src` must hold `src_len` bytes, `dst` must hold `dst_len` bytes; the buffers
/// must not overlap.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_lz4_decompress(
    src: *const c_void,
    src_len: usize,
    dst: *mut c_void,
    dst_len: usize,
) -> usize {
    let src = input(src, src_len);
    let dst = output(dst, dst_len);
    match lz4_flex::block::decompress_into(src, dst) {
        Ok(written) if written == dst_len => written,
        _ => usize::MAX,
    }
}

// ─── archives ──────────────────────────────────────────────────────────────

/// The stream an archive is read from: callbacks into the C++ `tTJSBinaryStream`.
#[repr(C)]
pub struct KrkrStream {
    pub userdata: *mut c_void,
    /// Reads at most `len` bytes; the count, 0 at the end, or a negative number
    /// on error.
    pub read: Option<unsafe extern "C" fn(*mut c_void, *mut c_void, usize) -> i64>,
    /// `whence` is 0 (start), 1 (current) or 2 (end); the new absolute
    /// position, or a negative number on error.
    pub seek: Option<unsafe extern "C" fn(*mut c_void, i64, i32) -> i64>,
    /// Length of the stream, needed to seek from the end.
    pub size: i64,
}

/// `Read + Seek` over [`KrkrStream`]: what lets [`crate::archives::Archive`]
/// read a `tTJSBinaryStream` it does not own.
pub struct Stream {
    userdata: *mut c_void,
    read: unsafe extern "C" fn(*mut c_void, *mut c_void, usize) -> i64,
    seek: unsafe extern "C" fn(*mut c_void, i64, i32) -> i64,
    size: u64,
}

// The pointers belong to the C++ side, which only ever runs one of these calls
// at a time (the archive holds its decoder behind a mutex).
unsafe impl Send for Stream {}

impl Stream {
    /// # Safety
    /// `stream` must point at a valid [`KrkrStream`] whose callbacks stay alive
    /// and callable until the archive is closed.
    unsafe fn from_c(stream: *const KrkrStream) -> Option<Self> {
        let stream = stream.as_ref()?;
        Some(Self {
            userdata: stream.userdata,
            read: stream.read?,
            seek: stream.seek?,
            size: if stream.size > 0 {
                stream.size as u64
            } else {
                0
            },
        })
    }
}

impl Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let n = unsafe { (self.read)(self.userdata, buf.as_mut_ptr().cast(), buf.len()) };
        if n < 0 {
            return Err(io::Error::other("archive stream read failed"));
        }
        // A callback that reports more than it was given would make the
        // decoders read uninitialised bytes; clamp instead of trusting it.
        Ok((n as usize).min(buf.len()))
    }
}

impl Seek for Stream {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let (offset, whence) = match pos {
            SeekFrom::Start(n) => (n as i64, 0),
            SeekFrom::Current(n) => (n, 1),
            SeekFrom::End(n) => ((self.size as i64).saturating_add(n), 0),
        };
        let position = unsafe { (self.seek)(self.userdata, offset, whence) };
        if position < 0 {
            return Err(io::Error::other("archive stream seek failed"));
        }
        Ok(position as u64)
    }
}

/// Opaque to C++; the layout is not part of the ABI.
pub type KrkrArchive = Archive<Stream>;

/// Where [`krkr_archive_open`] reports the entry list.
#[repr(C)]
pub struct KrkrArchiveEntries {
    pub userdata: *mut c_void,
    /// `index` is what [`krkr_archive_read`] takes. `name` is UTF-8,
    /// NUL-terminated, and lives only for the call. As with every visitor here,
    /// it runs on this crate's stack: it must not throw and must not call back
    /// into the shim.
    pub entry: Option<unsafe extern "C" fn(*mut c_void, usize, *const c_char, usize)>,
}

/// `KRKR_ARCHIVE_*` in `environ/bevy/compat/krkr_shim.h`.
pub const KRKR_ARCHIVE_7Z: i32 = 1;
pub const KRKR_ARCHIVE_ZIP: i32 = 2;

fn kind_from_c(kind: i32) -> Option<Kind> {
    match kind {
        KRKR_ARCHIVE_7Z => Some(Kind::SevenZ),
        KRKR_ARCHIVE_ZIP => Some(Kind::Zip),
        _ => None,
    }
}

/// Null when the stream is not an archive of `kind` or holds no files; the
/// caller then tries the next format, so the stream is left seekable. Entries
/// are reported through `entries` as the archive lists them.
///
/// # Safety
/// `stream` must be valid for the call and stay valid (its `userdata` and
/// callbacks) until [`krkr_archive_close`]; `entries` must be readable and its
/// callback must obey [`KrkrArchiveEntries`]'s rules.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_archive_open(
    stream: *const KrkrStream,
    kind: i32,
    entries: *const KrkrArchiveEntries,
) -> *mut KrkrArchive {
    guard(std::ptr::null_mut(), || {
        let (Some(stream), Some(kind)) = (Stream::from_c(stream), kind_from_c(kind)) else {
            return std::ptr::null_mut();
        };
        let Some(archive) = Archive::open(kind, stream) else {
            return std::ptr::null_mut();
        };
        if let Some(entries) = entries.as_ref()
            && let Some(entry) = entries.entry
        {
            for (index, listed) in archive.entries().iter().enumerate() {
                let name = c_string(listed.name());
                entry(
                    entries.userdata,
                    index,
                    name.as_ptr(),
                    listed.size() as usize,
                );
            }
        }

        Box::into_raw(Box::new(archive))
    })
}

/// Frees an archive; the stream itself stays with the caller.
///
/// # Safety
/// `archive` must come from [`krkr_archive_open`] and not be used afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_archive_close(archive: *mut KrkrArchive) {
    if archive.is_null() {
        return;
    }
    guard((), || drop(Box::from_raw(archive)));
}

/// Decompresses entry `index` into `buf` and returns the bytes written, or
/// [`FAILED`] when the entry does not fit or cannot be decoded.
///
/// # Safety
/// `archive` must be live; `buf` must hold `len` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_archive_read(
    archive: *mut KrkrArchive,
    index: usize,
    buf: *mut c_void,
    len: usize,
) -> usize {
    guard(FAILED, || {
        let Some(archive) = archive.as_ref() else {
            return FAILED;
        };
        let out = output(buf, len);
        match archive.read(index, out) {
            Some(written) => written,
            None => FAILED,
        }
    })
}

// ─── WebP ──────────────────────────────────────────────────────────────────
//
// libwebp's only user was `visual/LoadWEBP.cpp`, whose handler registers no save
// function, so the bitstream is decoded here and the engine only asks for a
// header and then for pixels.

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

// ─── JPEG ──────────────────────────────────────────────────────────────────
//
// `visual/LoadJPEG.cpp` is the only user: the file is decoded here, and the
// engine asks for a header, for pixels in the row layout of the bitmap it
// allocated, or for an encoded file.

/// 0 on success, -1 when `src` is not a JPEG. Fills the size only.
///
/// # Safety
/// `src` must hold `src_len` bytes (or be null if `src_len` is 0); `width` and
/// `height` must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_jpeg_info(
    src: *const c_void,
    src_len: usize,
    width: *mut c_uint,
    height: *mut c_uint,
) -> i32 {
    let (Some(width), Some(height)) = (width.as_mut(), height.as_mut()) else {
        return -1;
    };
    guard(-1, || {
        let Some((w, h)) = crate::jpeg::size(input(src, src_len)) else {
            return -1;
        };
        (*width, *height) = (w, h);
        0
    })
}

/// Writes four bytes per pixel, or one when `gray` is non-zero, and returns the
/// count, or [`FAILED`] when the image does not decode or does not fit
/// `dst_len`.
///
/// # Safety
/// `src` must hold `src_len` bytes (or be null if `src_len` is 0); `dst` must
/// hold `dst_len` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_jpeg_decode(
    src: *const c_void,
    src_len: usize,
    gray: i32,
    dst: *mut c_void,
    dst_len: usize,
) -> usize {
    let dst = output(dst, dst_len);
    guard(FAILED, || {
        let Some((_, _, pixels)) = crate::jpeg::decode(input(src, src_len), gray != 0) else {
            return FAILED;
        };
        if pixels.len() > dst.len() {
            return FAILED;
        }
        dst[..pixels.len()].copy_from_slice(&pixels);
        pixels.len()
    })
}

/// Encodes four byte pixels into a new JPEG, returned through `jpeg` and
/// `jpeg_len` and released with [`krkr_jpeg_free`]. 0 on success.
///
/// # Safety
/// `pixels` must hold `pixels_len` bytes; `jpeg` and `jpeg_len` must be
/// writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_jpeg_encode(
    pixels: *const c_void,
    pixels_len: usize,
    width: u32,
    height: u32,
    quality: i32,
    subsampling: i32,
    jpeg: *mut *mut c_void,
    jpeg_len: *mut usize,
) -> i32 {
    let (Some(jpeg), Some(jpeg_len)) = (jpeg.as_mut(), jpeg_len.as_mut()) else {
        return -1;
    };
    guard(-1, || {
        let quality = quality.clamp(1, 100) as u8;
        let Some(encoded) =
            crate::jpeg::encode(input(pixels, pixels_len), width, height, quality, subsampling)
        else {
            return -1;
        };
        *jpeg_len = encoded.len();
        *jpeg = Box::into_raw(encoded.into_boxed_slice()) as *mut u8 as *mut c_void;
        0
    })
}

/// # Safety
/// `ptr` must come from [`krkr_jpeg_encode`], with the `len` it reported.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_jpeg_free(ptr: *mut c_void, len: usize) {
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

// ─── PNG ───────────────────────────────────────────────────────────────────
//
// `visual/LoadPNG.cpp` is the only user: the file is decoded here, and the
// engine asks for a header, for pixels in the row layout of the bitmap it
// allocated, or for an encoded file.

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

// ─── zlib ──────────────────────────────────────────────────────────────────
//
// Only the one-shot zlib format calls are needed: XP3 indices and segments,
// `TextStream`'s compressed mode, the embedded option descriptions, and the
// `ZLIB_*` functions plugins call.

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

// ─── config files ──────────────────────────────────────────────────────────
//
// `environ/ConfigManager` keeps its settings and its locale table in flat XML
// lists; see `crate::xml`.

/// `KRKR_PREFERENCE_*` in `krkr_shim.h`.
pub const KRKR_PREFERENCE_ITEM: i32 = 0;
pub const KRKR_PREFERENCE_CUSTOM: i32 = 1;
pub const KRKR_PREFERENCE_KEYMAP: i32 = 2;

/// # Safety
/// `ptr` must be null or point at a NUL-terminated string.
unsafe fn text<'a>(ptr: *const c_char) -> Option<&'a str> {
    if ptr.is_null() {
        return None;
    }
    std::ffi::CStr::from_ptr(ptr).to_str().ok()
}

/// A path is bytes on Unix — the engine's directories can hold names that are
/// not UTF-8 — so it does not go through [`text`].
///
/// # Safety
/// `ptr` must be null or point at a NUL-terminated string.
unsafe fn path(ptr: *const c_char) -> Option<std::path::PathBuf> {
    if ptr.is_null() {
        return None;
    }
    let bytes = std::ffi::CStr::from_ptr(ptr).to_bytes();
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        Some(std::path::Path::new(std::ffi::OsStr::from_bytes(bytes)).to_path_buf())
    }
    #[cfg(not(unix))]
    {
        Some(std::path::PathBuf::from(std::str::from_utf8(bytes).ok()?))
    }
}

/// Callbacks for [`krkr_preference_read`]. They run on this crate's stack, so a
/// C++ exception escaping one aborts the process; they must not call back into
/// the shim either. Both strings are UTF-8 and NUL-terminated, and live only
/// for the call.
#[repr(C)]
pub struct KrkrPreferenceVisitor {
    pub userdata: *mut c_void,
    /// A setting or an extra command-line argument, `list` being
    /// `KRKR_PREFERENCE_ITEM` or `KRKR_PREFERENCE_CUSTOM`.
    pub entry: Option<unsafe extern "C" fn(*mut c_void, i32, *const c_char, *const c_char)>,
    /// A key map entry, already read as numbers.
    pub keymap: Option<unsafe extern "C" fn(*mut c_void, i32, i32)>,
}

/// Callback for [`krkr_locale_read`]; same rules as [`KrkrPreferenceVisitor`].
#[repr(C)]
pub struct KrkrTextVisitor {
    pub userdata: *mut c_void,
    pub text: Option<unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char)>,
}

/// One record for [`krkr_preference_write`].
#[repr(C)]
pub struct KrkrPreferenceRecord {
    /// `KRKR_PREFERENCE_ITEM`, `KRKR_PREFERENCE_CUSTOM` or `KRKR_PREFERENCE_KEYMAP`.
    pub list: i32,
    /// Key map entries only: the key code before and after the remap.
    pub key: i32,
    pub value: i32,
    /// Settings and arguments only: UTF-8, NUL-terminated, read during the call.
    pub key_text: *const c_char,
    pub value_text: *const c_char,
}

/// A NUL-terminated copy to hand to a visitor. A string that is not UTF-8
/// becomes empty rather than failing the whole file.
fn c_string(text: &str) -> std::ffi::CString {
    std::ffi::CString::new(text).unwrap_or_default()
}

/// 0 when the file was read and is well-formed XML, -1 otherwise.
///
/// # Safety
/// `path_ptr` must be NUL-terminated; `visitor` must be readable and its
/// callbacks must obey [`KrkrPreferenceVisitor`]'s rules.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_preference_read(
    path_ptr: *const c_char,
    visitor: *const KrkrPreferenceVisitor,
) -> i32 {
    guard(-1, || {
        let (Some(path), Some(visitor)) = (path(path_ptr), visitor.as_ref()) else {
            return -1;
        };
        let Ok(xml) = std::fs::read(path) else {
            return -1;
        };
        let Some(preference) = xml::Preference::parse(&xml) else {
            return -1;
        };

        if let Some(entry) = visitor.entry {
            let settings = preference
                .items
                .iter()
                .map(|(key, value)| (KRKR_PREFERENCE_ITEM, key, value))
                .chain(
                    preference
                        .custom_arguments
                        .iter()
                        .map(|(key, value)| (KRKR_PREFERENCE_CUSTOM, key, value)),
                );
            for (list, key, value) in settings {
                let (key, value) = (c_string(key), c_string(value));
                entry(visitor.userdata, list, key.as_ptr(), value.as_ptr());
            }
        }
        if let Some(keymap) = visitor.keymap {
            for &(key, value) in &preference.keymap {
                keymap(visitor.userdata, key, value);
            }
        }
        0
    })
}

/// Writes the records in the order given, which is the order they are read
/// back in. 0 when the file was written, -1 when it could not be.
///
/// # Safety
/// `path_ptr` must be NUL-terminated; `records` must hold `count` readable
/// records whose strings are NUL-terminated (or be null when `count` is 0).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_preference_write(
    path_ptr: *const c_char,
    records: *const KrkrPreferenceRecord,
    count: usize,
) -> i32 {
    guard(-1, || {
        let Some(path) = path(path_ptr) else {
            return -1;
        };
        let records: &[KrkrPreferenceRecord] = if records.is_null() || count == 0 {
            &[]
        } else {
            std::slice::from_raw_parts(records, count)
        };
        let mut preference = xml::Preference::default();
        let record_text = |ptr: *const c_char| text(ptr).unwrap_or_default().to_owned();
        for record in records {
            match record.list {
                KRKR_PREFERENCE_ITEM => preference
                    .items
                    .push((record_text(record.key_text), record_text(record.value_text))),
                KRKR_PREFERENCE_CUSTOM => preference
                    .custom_arguments
                    .push((record_text(record.key_text), record_text(record.value_text))),
                KRKR_PREFERENCE_KEYMAP => preference.keymap.push((record.key, record.value)),
                _ => {}
            }
        }
        match std::fs::write(path, preference.to_xml()) {
            Ok(()) => 0,
            Err(_) => -1,
        }
    })
}

/// The message count; a document that is not well-formed XML has none.
///
/// The engine reads the file itself — it can live inside an archive — so the
/// contents come in rather than a path.
///
/// # Safety
/// `data` must hold `len` bytes (or be null if `len` is 0); `visitor` must be
/// readable and its callback must obey [`KrkrTextVisitor`]'s rules.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_locale_read(
    data: *const c_void,
    len: usize,
    visitor: *const KrkrTextVisitor,
) -> usize {
    guard(0, || {
        let Some(visitor) = visitor.as_ref() else {
            return 0;
        };
        let Some(text_callback) = visitor.text else {
            return 0;
        };
        let Some(locale) = xml::Locale::parse(input(data, len)) else {
            return 0;
        };
        for (id, text) in &locale.texts {
            let (id, text) = (c_string(id), c_string(text));
            text_callback(visitor.userdata, id.as_ptr(), text.as_ptr());
        }
        locale.texts.len()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hashes and LZ4 blocks are only correct if they match the upstream
    /// algorithms bit for bit; these vectors come from `xxhsum -H0` / `-H3`.
    const HASHES: &[(&[u8], u32, u64)] = &[
        (b"", 0x02cc_5d05, 0x2d06_8005_38d3_94c2),
        (b"a", 0x550d_7456, 0xe6c6_32b6_1e96_4e1f),
        (b"abc", 0x32d1_53ff, 0x78af_5f94_892f_3950),
        (
            b"The quick brown fox jumps over the lazy dog",
            0xe85e_a4de,
            0xce7d_19a5_418f_b365,
        ),
    ];

    #[test]
    fn xxhash_matches_upstream() {
        for &(data, xxh32, xxh3) in HASHES {
            let ptr = if data.is_empty() {
                std::ptr::null()
            } else {
                data.as_ptr().cast()
            };
            unsafe {
                assert_eq!(XXH32(ptr, data.len(), 0), xxh32, "XXH32({data:?})");
                assert_eq!(
                    XXH3_64bits_withSeed(ptr, data.len(), 0),
                    xxh3,
                    "XXH3({data:?})"
                );
            }
        }
    }

    #[test]
    fn xxh3_seed_changes_the_hash() {
        let data = b"frame";
        unsafe {
            assert_ne!(
                XXH3_64bits_withSeed(data.as_ptr().cast(), data.len(), 1),
                XXH3_64bits_withSeed(data.as_ptr().cast(), data.len(), 2)
            );
        }
    }

    /// Compresses `data` and decompresses it back through the C entry points.
    fn roundtrip(data: &[u8]) -> Vec<u8> {
        // `get_maximum_output_size` is the bound `LZ4_compressBound` gives.
        let mut compressed = vec![0u8; lz4_flex::block::get_maximum_output_size(data.len())];
        let written = unsafe {
            LZ4_compress_default(
                data.as_ptr().cast(),
                compressed.as_mut_ptr().cast(),
                data.len() as i32,
                compressed.len() as i32,
            )
        };
        assert!(written > 0, "compression of {} bytes failed", data.len());
        compressed.truncate(written as usize);

        let mut out = vec![0u8; data.len()];
        let written = unsafe {
            krkr_lz4_decompress(
                compressed.as_ptr().cast(),
                compressed.len(),
                out.as_mut_ptr().cast(),
                out.len(),
            )
        };
        assert_eq!(written, data.len());
        out
    }

    #[test]
    fn lz4_roundtrips() {
        // Lengths around the block-format edges: LZ4 has to emit the last bytes
        // as literals and refuses matches too close to the end, so the encoder
        // and decoder must agree on inputs of a few bytes as well as large
        // ones. Every length here is a texture band (pitch × rows) in practice.
        for &len in &[1, 4, 11, 12, 13, 63, 64, 65, 4096, 16384] {
            let data: Vec<u8> = (0..len).map(|i| (i * 7 % 251) as u8).collect();
            assert_eq!(roundtrip(&data), data, "len {len}");
        }
        // Incompressible data still has to survive.
        let noise: Vec<u8> = (0..8192u32)
            .map(|i| (i.wrapping_mul(2654435761) >> 24) as u8)
            .collect();
        assert_eq!(roundtrip(&noise), noise);
        // A fully transparent band compresses to almost nothing.
        let zeros = vec![0u8; 16384];
        assert_eq!(roundtrip(&zeros), zeros);
    }

    #[test]
    fn lz4_rejects_a_truncated_block() {
        let data = vec![7u8; 4096];
        let mut compressed = vec![0u8; lz4_flex::block::get_maximum_output_size(data.len())];
        let written = unsafe {
            LZ4_compress_default(
                data.as_ptr().cast(),
                compressed.as_mut_ptr().cast(),
                data.len() as i32,
                compressed.len() as i32,
            )
        } as usize;
        let mut out = vec![0u8; data.len()];
        let written = unsafe {
            krkr_lz4_decompress(
                compressed.as_ptr().cast(),
                written - 1,
                out.as_mut_ptr().cast(),
                out.len(),
            )
        };
        assert_eq!(written, usize::MAX, "a truncated block must not decode");
    }

    /// A C caller that passes nothing must get a failure, not a crash: the
    /// engine does pass null handles when a load was cancelled.
    #[test]
    fn null_handles_are_rejected() {
        unsafe {
            assert!(
                krkr_archive_open(std::ptr::null(), KRKR_ARCHIVE_7Z, std::ptr::null()).is_null()
            );
            assert_eq!(
                krkr_archive_read(std::ptr::null_mut(), 0, std::ptr::null_mut(), 0),
                usize::MAX
            );
            krkr_archive_close(std::ptr::null_mut()); // must be a no-op
        }
    }

    /// An unknown archive kind is an error, not a guess.
    #[test]
    fn an_unknown_kind_is_rejected() {
        // The stream is never read — the kind is rejected first — but it still
        // has to be well-formed, since `Stream` requires both callbacks.
        unsafe extern "C" fn read_nothing(_: *mut c_void, _: *mut c_void, _: usize) -> i64 {
            0
        }
        unsafe extern "C" fn seek_to_start(_: *mut c_void, _: i64, _: i32) -> i64 {
            0
        }
        let stream = KrkrStream {
            userdata: std::ptr::null_mut(),
            read: Some(read_nothing),
            seek: Some(seek_to_start),
            size: 0,
        };
        unsafe { assert!(krkr_archive_open(&stream, 0, std::ptr::null()).is_null()) };
    }
}
