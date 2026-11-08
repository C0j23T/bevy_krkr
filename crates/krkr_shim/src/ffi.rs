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

use std::ffi::c_void;


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
}
