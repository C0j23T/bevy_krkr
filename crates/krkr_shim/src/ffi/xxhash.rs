use std::ffi::c_void;

use super::input;

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

#[cfg(test)]
mod tests {
    use super::*;

    /// Hashes are only correct if they match the upstream algorithms bit for
    /// bit; these vectors come from `xxhsum -H0` / `-H3`.
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
}
