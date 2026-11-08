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

use std::ffi::{c_char, c_void};
use std::io::{self, Read, Seek, SeekFrom};
use std::panic::{catch_unwind, AssertUnwindSafe};

use crate::archives::{Archive, Kind};

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
#[no_mangle]
pub unsafe extern "C" fn XXH32(input_ptr: *const c_void, length: usize, seed: u32) -> u32 {
    xxhash_rust::xxh32::xxh32(input(input_ptr, length), seed)
}

/// `XXH64_hash_t XXH3_64bits_withSeed(const void*, size_t, XXH64_hash_t)`.
///
/// # Safety
/// `input_ptr` must point to `length` readable bytes (or be null if `length` is 0).
#[no_mangle]
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
#[no_mangle]
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
#[no_mangle]
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

/// Callbacks into the C++ `tTJSBinaryStream`. Mirrors `KrkrArchiveReader` in
/// `environ/bevy/compat/krkr_shim.h`.
#[repr(C)]
pub struct KrkrArchiveReader {
    pub userdata: *mut c_void,
    /// Reads at most `len` bytes; returns the count, 0 at end of stream, or a
    /// negative number on error.
    pub read: Option<unsafe extern "C" fn(*mut c_void, *mut c_void, usize) -> i64>,
    /// `whence` is 0 (start), 1 (current) or 2 (end); returns the new absolute
    /// position, or a negative number on error.
    pub seek: Option<unsafe extern "C" fn(*mut c_void, i64, i32) -> i64>,
    /// Length of the stream, needed to seek from the end.
    pub size: i64,
}

/// `Read + Seek` over the C callbacks: the adapter that lets
/// [`crate::archives::Archive`] read from a `tTJSBinaryStream` it does not own.
///
/// Public only because it is the type behind the `KrkrArchive *` handle; its
/// fields and the type itself are not part of the ABI the C++ side sees.
pub struct Source {
    userdata: *mut c_void,
    read: unsafe extern "C" fn(*mut c_void, *mut c_void, usize) -> i64,
    seek: unsafe extern "C" fn(*mut c_void, i64, i32) -> i64,
    size: u64,
}

// The pointers belong to the C++ side, which only ever runs one of these calls
// at a time (the archive holds the decoder behind a mutex).
unsafe impl Send for Source {}

impl Source {
    /// # Safety
    /// `reader` must point at a valid `KrkrArchiveReader` whose callbacks stay
    /// alive and callable until the archive is closed.
    unsafe fn from_c(reader: *const KrkrArchiveReader) -> Option<Self> {
        let reader = reader.as_ref()?;
        Some(Self {
            userdata: reader.userdata,
            read: reader.read?,
            seek: reader.seek?,
            size: if reader.size > 0 { reader.size as u64 } else { 0 },
        })
    }
}

impl Read for Source {
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

impl Seek for Source {
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

/// The handle the C++ side holds as `KrkrArchive *`. Opaque to it: the layout
/// is not part of the ABI.
pub type KrkrArchive = Archive<Source>;

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

/// Opens `reader` as a 7z or ZIP archive and lists its entries.
///
/// Returns null when the stream is not an archive of `kind`; the caller
/// (`TVPOpenArchive`) then tries the next format, so this must leave the
/// stream seekable.
///
/// # Safety
/// `reader` must be valid for the call and stay valid (its `userdata` and
/// callbacks) until [`krkr_archive_close`].
#[no_mangle]
pub unsafe extern "C" fn krkr_archive_open(
    reader: *const KrkrArchiveReader,
    kind: i32,
) -> *mut KrkrArchive {
    guard(std::ptr::null_mut(), || {
        let (Some(source), Some(kind)) = (Source::from_c(reader), kind_from_c(kind)) else {
            return std::ptr::null_mut();
        };
        match Archive::open(kind, source) {
            Some(archive) => Box::into_raw(Box::new(archive)),
            None => std::ptr::null_mut(),
        }
    })
}

/// Frees an archive; the stream itself stays with the caller.
///
/// # Safety
/// `archive` must come from [`krkr_archive_open`] and not be used afterwards.
#[no_mangle]
pub unsafe extern "C" fn krkr_archive_close(archive: *mut KrkrArchive) {
    if archive.is_null() {
        return;
    }
    guard((), || drop(Box::from_raw(archive)));
}

/// Number of entries in the list.
///
/// # Safety
/// `archive` must be a live archive handle.
#[no_mangle]
pub unsafe extern "C" fn krkr_archive_count(archive: *const KrkrArchive) -> usize {
    archive.as_ref().map_or(0, |archive| archive.entries().len())
}

/// Uncompressed size of entry `index`, or `SIZE_MAX` for a bad index.
///
/// # Safety
/// `archive` must be a live archive handle.
#[no_mangle]
pub unsafe extern "C" fn krkr_archive_size(archive: *const KrkrArchive, index: usize) -> usize {
    match archive.as_ref().and_then(|archive| archive.entries().get(index)) {
        Some(entry) => entry.size() as usize,
        None => FAILED,
    }
}

/// Copies entry `index`'s name into `buf` as UTF-8 and NUL-terminates it.
///
/// Returns the byte count without the NUL. `buf` may be null (or `len` 0) to
/// ask for the length only, which is how the caller sizes its buffer.
///
/// # Safety
/// `archive` must be live; `buf` must hold `len` writable bytes when not null.
#[no_mangle]
pub unsafe extern "C" fn krkr_archive_name(
    archive: *const KrkrArchive,
    index: usize,
    buf: *mut c_char,
    len: usize,
) -> usize {
    let Some(entry) = archive.as_ref().and_then(|archive| archive.entries().get(index)) else {
        return FAILED;
    };
    let name = entry.name().as_bytes();
    if !buf.is_null() && len > 0 {
        let copied = name.len().min(len - 1);
        std::ptr::copy_nonoverlapping(name.as_ptr(), buf.cast(), copied);
        *buf.add(copied) = 0;
    }
    name.len()
}

/// Decompresses entry `index` into `buf` (`len` bytes).
///
/// Returns the bytes written, or `SIZE_MAX` when the entry does not fit or
/// cannot be decoded. The caller gets the exact size from [`krkr_archive_size`]
/// first, so a short read is an error rather than something to pad.
///
/// # Safety
/// `archive` must be live; `buf` must hold `len` writable bytes.
#[no_mangle]
pub unsafe extern "C" fn krkr_archive_read(
    archive: *mut KrkrArchive,
    index: usize,
    buf: *mut c_void,
    len: usize,
) -> usize {
    guard(FAILED, || {
        let Some(archive) = archive.as_ref() else { return FAILED };
        let out = output(buf, len);
        match archive.read(index, out) {
            Some(written) => written,
            None => FAILED,
        }
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
            let ptr = if data.is_empty() { std::ptr::null() } else { data.as_ptr().cast() };
            unsafe {
                assert_eq!(XXH32(ptr, data.len(), 0), xxh32, "XXH32({data:?})");
                assert_eq!(XXH3_64bits_withSeed(ptr, data.len(), 0), xxh3, "XXH3({data:?})");
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
        let noise: Vec<u8> =
            (0..8192u32).map(|i| (i.wrapping_mul(2654435761) >> 24) as u8).collect();
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
            assert!(krkr_archive_open(std::ptr::null(), KRKR_ARCHIVE_7Z).is_null());
            assert_eq!(krkr_archive_count(std::ptr::null()), 0);
            assert_eq!(krkr_archive_size(std::ptr::null(), 0), usize::MAX);
            assert_eq!(krkr_archive_name(std::ptr::null(), 0, std::ptr::null_mut(), 0), usize::MAX);
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
        // The stream is never reached — the kind is rejected first — but the
        // reader still has to be well-formed, since `Source` requires both
        // callbacks.
        unsafe extern "C" fn read_nothing(_: *mut c_void, _: *mut c_void, _: usize) -> i64 {
            0
        }
        unsafe extern "C" fn seek_to_start(_: *mut c_void, _: i64, _: i32) -> i64 {
            0
        }
        let reader = KrkrArchiveReader {
            userdata: std::ptr::null_mut(),
            read: Some(read_nothing),
            seek: Some(seek_to_start),
            size: 0,
        };
        unsafe { assert!(krkr_archive_open(&reader, 0).is_null()) };
    }
}
