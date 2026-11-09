//! The zlib entry points through the C ABI, the way XP3 and the text streams
//! drive them: the bound, then one compress or uncompress into a buffer the
//! caller sized.

use krkr_shim::ffi::{krkr_zlib_bound, krkr_zlib_compress, krkr_zlib_uncompress};

const FAILED: usize = usize::MAX;

fn compress(data: &[u8], level: i32) -> Option<Vec<u8>> {
    let mut out = vec![0u8; krkr_zlib_bound(data.len())];
    let written = unsafe {
        krkr_zlib_compress(
            out.as_mut_ptr().cast(),
            out.len(),
            data.as_ptr().cast(),
            data.len(),
            level,
        )
    };
    (written != FAILED).then(|| {
        out.truncate(written);
        out
    })
}

fn uncompress(data: &[u8], len: usize) -> Option<Vec<u8>> {
    let mut out = vec![0u8; len];
    let written = unsafe {
        krkr_zlib_uncompress(
            out.as_mut_ptr().cast(),
            out.len(),
            data.as_ptr().cast(),
            data.len(),
        )
    };
    (written != FAILED).then(|| {
        out.truncate(written);
        out
    })
}

#[test]
fn round_trips_at_every_level() {
    let data: Vec<u8> = (0..10_000u32).map(|i| (i % 251) as u8).collect();
    for level in 0..=9 {
        let packed = compress(&data, level).expect("compresses");
        assert!(packed.len() <= krkr_zlib_bound(data.len()));
        assert_eq!(uncompress(&packed, data.len()), Some(data.clone()), "level {level}");
    }
}

#[test]
fn round_trips_nothing() {
    let packed = compress(&[], -1).expect("compresses");
    assert_eq!(uncompress(&packed, 0), Some(Vec::new()));
}

#[test]
fn a_short_destination_fails() {
    let data = b"the quick brown fox jumps over the lazy dog".repeat(20);
    let packed = compress(&data, 6).expect("compresses");

    assert_eq!(uncompress(&packed, data.len() - 1), None);
    assert_eq!(uncompress(&packed, data.len()), Some(data));
}

#[test]
fn garbage_is_rejected() {
    assert_eq!(uncompress(b"not a zlib stream", 100), None);
    assert_eq!(uncompress(b"", 100), None);
}
