use std::ffi::{c_char, c_void};
use std::io::{self, Read, Seek, SeekFrom};

use crate::archives::{Archive, Kind};

use super::{FAILED, c_string, guard, output};

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

#[cfg(test)]
mod tests {
    use super::*;

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
                FAILED
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
