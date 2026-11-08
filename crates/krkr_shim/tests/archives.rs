//! The 7z / ZIP reader, driven through the C ABI (`crates/krkr_shim/src/ffi.rs`)
//! the way `environ/bevy/BevyArchives.cpp` drives it.
//!
//! What this file is for is the seam: callbacks, the name copy, the sentinels
//! and the "leave the stream usable" contract `TVPOpenArchive` depends on. How
//! the formats are parsed is tested in `src/archives.rs`, without any of this.
//!
//! The fixtures are real archives: `sample.7z` (LZMA2, what games ship) and
//! `sample.zip` (deflate), each holding three files — one at the root, one in a
//! subdirectory, one with a Japanese name — plus the directory entries. Names
//! exercise the UTF-16 → UTF-8 path that used to depend on the process locale.

use std::ffi::c_void;

use krkr_shim::ffi::{
    krkr_archive_close, krkr_archive_count, krkr_archive_name, krkr_archive_open, krkr_archive_read,
    krkr_archive_size, KrkrArchiveReader, KRKR_ARCHIVE_7Z, KRKR_ARCHIVE_ZIP,
};

const SEVEN_Z: &[u8] = include_bytes!("data/sample.7z");
const ZIP: &[u8] = include_bytes!("data/sample.zip");

/// A stream over a byte slice, standing in for the engine's
/// `tTJSBinaryStream`. `pos` is the only state, exactly like the C++ side.
struct SliceStream {
    data: &'static [u8],
    pos: usize,
}

unsafe extern "C" fn read_cb(userdata: *mut c_void, buf: *mut c_void, len: usize) -> i64 {
    let stream = &mut *(userdata as *mut SliceStream);
    let count = len.min(stream.data.len() - stream.pos);
    if count > 0 {
        std::ptr::copy_nonoverlapping(stream.data.as_ptr().add(stream.pos), buf.cast::<u8>(), count);
        stream.pos += count;
    }
    count as i64
}

unsafe extern "C" fn seek_cb(userdata: *mut c_void, offset: i64, whence: i32) -> i64 {
    let stream = &mut *(userdata as *mut SliceStream);
    let base = match whence {
        0 => 0,
        1 => stream.pos as i64,
        _ => stream.data.len() as i64,
    };
    let target = base + offset;
    if target < 0 || target > stream.data.len() as i64 {
        return -1;
    }
    stream.pos = target as usize;
    target
}

fn reader_for(stream: &mut SliceStream) -> KrkrArchiveReader {
    KrkrArchiveReader {
        userdata: (stream as *mut SliceStream).cast(),
        read: Some(read_cb),
        seek: Some(seek_cb),
        size: stream.data.len() as i64,
    }
}

/// Entry names of an archive, in the order the archive lists them.
fn names(data: &'static [u8], kind: i32) -> Option<Vec<String>> {
    let mut stream = SliceStream { data, pos: 0 };
    let reader = reader_for(&mut stream);
    unsafe {
        let archive = krkr_archive_open(&reader, kind);
        if archive.is_null() {
            return None;
        }
        let count = krkr_archive_count(archive);
        let mut out = Vec::with_capacity(count);
        for index in 0..count {
            // The length-only call is what the C++ side uses to size its buffer.
            let len = krkr_archive_name(archive, index, std::ptr::null_mut(), 0);
            assert_ne!(len, usize::MAX);
            let mut buf = vec![0u8; len + 1];
            let copied = krkr_archive_name(archive, index, buf.as_mut_ptr().cast(), buf.len());
            assert_eq!(copied, len);
            assert_eq!(buf[len], 0, "the name must be NUL-terminated");
            buf.truncate(len);
            out.push(String::from_utf8(buf).expect("names are UTF-8"));
        }
        krkr_archive_close(archive);
        Some(out)
    }
}

/// Reads every entry into `(name, contents)`, checking the sizes agree.
fn contents(data: &'static [u8], kind: i32) -> Vec<(String, Vec<u8>)> {
    let mut stream = SliceStream { data, pos: 0 };
    let reader = reader_for(&mut stream);
    unsafe {
        let archive = krkr_archive_open(&reader, kind);
        assert!(!archive.is_null());
        let mut out = Vec::new();
        for index in 0..krkr_archive_count(archive) {
            let len = krkr_archive_name(archive, index, std::ptr::null_mut(), 0);
            let mut name = vec![0u8; len + 1];
            krkr_archive_name(archive, index, name.as_mut_ptr().cast(), name.len());
            name.truncate(len);

            let size = krkr_archive_size(archive, index);
            assert_ne!(size, usize::MAX);
            let mut buf = vec![0u8; size];
            let written = krkr_archive_read(archive, index, buf.as_mut_ptr().cast(), buf.len());
            assert_eq!(written, size, "entry {index} did not decompress to its declared size");
            out.push((String::from_utf8(name).unwrap(), buf));
        }
        krkr_archive_close(archive);
        out
    }
}

fn expected() -> Vec<(&'static str, &'static [u8])> {
    vec![
        ("hello.txt", b"hello, 7z\n"),
        ("日本語/セリフ.txt", "セリフです\n".as_bytes()),
        ("sub/deep.txt", b"nested\n"),
    ]
}

/// Entry lists match, and directories are not entries (the engine keeps its own
/// directory tree).
fn check_listing(kind: i32, data: &'static [u8]) {
    let mut listed = names(data, kind).expect("the archive opens");
    listed.sort();
    let mut want: Vec<String> = expected().iter().map(|(name, _)| name.to_string()).collect();
    want.sort();
    assert_eq!(listed, want);
}

#[test]
fn seven_z_lists_its_files() {
    check_listing(KRKR_ARCHIVE_7Z, SEVEN_Z);
}

#[test]
fn zip_lists_its_files() {
    check_listing(KRKR_ARCHIVE_ZIP, ZIP);
}

/// Every entry decompresses to exactly its declared size and content. A wrong
/// size would make `CreateStreamByIndex` hand the engine a truncated stream.
fn check_contents(kind: i32, data: &'static [u8]) {
    let read = contents(data, kind);
    for (name, want) in expected() {
        let found = read
            .iter()
            .find(|(entry, _)| entry == name)
            .unwrap_or_else(|| panic!("{name} missing from {:?}", read.iter().map(|r| &r.0).collect::<Vec<_>>()));
        assert_eq!(found.1, want, "{name} has the wrong contents");
    }
}

#[test]
fn seven_z_reads_every_entry() {
    check_contents(KRKR_ARCHIVE_7Z, SEVEN_Z);
}

#[test]
fn zip_reads_every_entry() {
    check_contents(KRKR_ARCHIVE_ZIP, ZIP);
}

/// `TVPOpenArchive` tries one opener after another on the same stream, so an
/// opener must reject a format that is not its own — without moving the stream
/// and without crashing.
#[test]
fn each_kind_rejects_the_other_format() {
    assert_eq!(names(SEVEN_Z, KRKR_ARCHIVE_ZIP), None, "zip opener took a 7z");
    assert_eq!(names(ZIP, KRKR_ARCHIVE_7Z), None, "7z opener took a zip");
}

/// A rejected stream has to be left where it was: the next opener reads from
/// the current position, and the engine only rewinds once per format.
#[test]
fn a_rejected_stream_is_left_seekable() {
    let mut stream = SliceStream { data: ZIP, pos: 0 };
    let reader = reader_for(&mut stream);
    unsafe {
        let archive = krkr_archive_open(&reader, KRKR_ARCHIVE_7Z);
        assert!(archive.is_null());
        krkr_archive_close(archive);
    }
    // The same stream still opens as what it is.
    let mut stream = stream;
    let reader = reader_for(&mut stream);
    unsafe {
        let archive = krkr_archive_open(&reader, KRKR_ARCHIVE_ZIP);
        assert!(!archive.is_null(), "the stream is no longer usable");
        krkr_archive_close(archive);
    }
}

/// Out-of-range indices report failure instead of reading past the entry list.
#[test]
fn a_bad_index_fails() {
    let mut stream = SliceStream { data: SEVEN_Z, pos: 0 };
    let reader = reader_for(&mut stream);
    unsafe {
        let archive = krkr_archive_open(&reader, KRKR_ARCHIVE_7Z);
        assert!(!archive.is_null());
        let out_of_range = krkr_archive_count(archive);
        assert_eq!(krkr_archive_size(archive, out_of_range), usize::MAX);
        assert_eq!(krkr_archive_name(archive, out_of_range, std::ptr::null_mut(), 0), usize::MAX);
        let mut buf = [0u8; 16];
        assert_eq!(krkr_archive_read(archive, out_of_range, buf.as_mut_ptr().cast(), buf.len()), usize::MAX);
        krkr_archive_close(archive);
    }
}

/// A buffer too small for the entry is an error, not a truncated copy: the
/// caller allocates from `krkr_archive_size`, so a mismatch means it mis-sized.
#[test]
fn a_short_buffer_is_refused() {
    let mut stream = SliceStream { data: SEVEN_Z, pos: 0 };
    let reader = reader_for(&mut stream);
    unsafe {
        let archive = krkr_archive_open(&reader, KRKR_ARCHIVE_7Z);
        assert!(!archive.is_null());
        let index = (0..krkr_archive_count(archive))
            .find(|&i| krkr_archive_size(archive, i) == 10)
            .expect("hello.txt is 10 bytes");
        let mut buf = [0u8; 4];
        assert_eq!(krkr_archive_read(archive, index, buf.as_mut_ptr().cast(), buf.len()), usize::MAX);
        krkr_archive_close(archive);
    }
}
