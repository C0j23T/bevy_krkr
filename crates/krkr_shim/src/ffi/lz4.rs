use std::ffi::c_void;

use super::{FAILED, input, output};

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
/// size. Returns the number of bytes written, or [`FAILED`] when the block is
/// malformed or does not produce exactly `dst_len` bytes.
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
        _ => FAILED,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(written, FAILED, "a truncated block must not decode");
    }
}
