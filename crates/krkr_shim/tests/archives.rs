#![allow(unsafe_op_in_unsafe_fn)]

//! The 7z / ZIP reader through the C ABI, the way `environ/bevy/Archives.cpp`
//! drives it: the stream callbacks, the entry visitor, the sentinels, and the
//! "leave the stream usable" contract `TVPOpenArchive` depends on. The formats
//! themselves are tested in `src/archives.rs`.
//!
//! The fixtures are real archives: `sample.7z` (LZMA2, what games ship) and
//! `sample.zip` (deflate), each holding three files — one at the root, one in a
//! subdirectory, one with a Japanese name — plus the directory entries.

use std::ffi::{c_char, c_void, CStr};

use krkr_shim::ffi::{
    krkr_archive_close, krkr_archive_open, krkr_archive_read, KrkrArchive, KrkrArchiveEntries,
    KrkrStream, KRKR_ARCHIVE_7Z, KRKR_ARCHIVE_ZIP,
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

fn stream_for(stream: &mut SliceStream) -> KrkrStream {
    KrkrStream {
        userdata: (stream as *mut SliceStream).cast(),
        read: Some(read_cb),
        seek: Some(seek_cb),
        size: stream.data.len() as i64,
    }
}

unsafe extern "C" fn collect_entry(
    userdata: *mut c_void,
    index: usize,
    name: *const c_char,
    size: usize,
) {
    let listed = &mut *(userdata as *mut Vec<(String, usize)>);
    assert_eq!(listed.len(), index, "the visitor index must be the read index");
    listed.push((CStr::from_ptr(name).to_str().expect("names are UTF-8").to_owned(), size));
}

/// `(handle, entries as the visitor reported them)`; `None` when the stream is
/// not an archive of `kind`.
fn open_listed(
    stream: &mut SliceStream,
    kind: i32,
) -> Option<(*mut KrkrArchive, Vec<(String, usize)>)> {
    let mut listed = Vec::new();
    let stream = stream_for(stream);
    let entries = KrkrArchiveEntries {
        userdata: (&mut listed as *mut Vec<(String, usize)>).cast(),
        entry: Some(collect_entry),
    };
    let archive = unsafe { krkr_archive_open(&stream, kind, &entries) };
    if archive.is_null() {
        assert!(listed.is_empty(), "a rejected stream must report no entries");
        return None;
    }
    Some((archive, listed))
}

/// Entry names, in the order the archive lists them.
fn names(data: &'static [u8], kind: i32) -> Option<Vec<String>> {
    let mut stream = SliceStream { data, pos: 0 };
    let (archive, listed) = open_listed(&mut stream, kind)?;
    unsafe { krkr_archive_close(archive) };
    Some(listed.into_iter().map(|(name, _)| name).collect())
}

/// Every entry, read at the index the listing gave, into `(name, contents)`.
fn contents(data: &'static [u8], kind: i32) -> Vec<(String, Vec<u8>)> {
    let mut stream = SliceStream { data, pos: 0 };
    let (archive, listed) = open_listed(&mut stream, kind).expect("the archive opens");
    let mut out = Vec::new();
    for (index, (name, size)) in listed.into_iter().enumerate() {
        let mut buf = vec![0u8; size];
        let written = unsafe { krkr_archive_read(archive, index, buf.as_mut_ptr().cast(), buf.len()) };
        assert_eq!(written, size, "entry {index} did not decompress to its declared size");
        out.push((name, buf));
    }
    unsafe { krkr_archive_close(archive) };
    out
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
    assert!(open_listed(&mut stream, KRKR_ARCHIVE_7Z).is_none());

    let (archive, listed) = open_listed(&mut stream, KRKR_ARCHIVE_ZIP)
        .expect("the stream is no longer usable");
    assert_eq!(listed.len(), 3);
    unsafe { krkr_archive_close(archive) };
}

/// Out-of-range indices report failure instead of reading past the entry list.
#[test]
fn a_bad_index_fails() {
    let mut stream = SliceStream { data: SEVEN_Z, pos: 0 };
    let (archive, listed) = open_listed(&mut stream, KRKR_ARCHIVE_7Z).expect("opens");
    let mut buf = [0u8; 16];
    let written = unsafe {
        krkr_archive_read(archive, listed.len(), buf.as_mut_ptr().cast(), buf.len())
    };
    assert_eq!(written, usize::MAX);
    unsafe { krkr_archive_close(archive) };
}

/// A buffer too small for the entry is an error, not a truncated copy: the
/// caller allocates from the size the listing gave, so a mismatch means it
/// mis-sized.
#[test]
fn a_short_buffer_is_refused() {
    let mut stream = SliceStream { data: SEVEN_Z, pos: 0 };
    let (archive, listed) = open_listed(&mut stream, KRKR_ARCHIVE_7Z).expect("opens");
    let index = listed
        .iter()
        .position(|(_, size)| *size == 10)
        .expect("hello.txt is 10 bytes");
    let mut buf = [0u8; 4];
    let written = unsafe { krkr_archive_read(archive, index, buf.as_mut_ptr().cast(), buf.len()) };
    assert_eq!(written, usize::MAX);
    unsafe { krkr_archive_close(archive) };
}
