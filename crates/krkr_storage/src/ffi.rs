//! C ABI. Pointers must be valid for their declared lengths; handles are exclusive
//! and must be closed exactly once. No callback or exception crosses Rust frames.
#![allow(unsafe_op_in_unsafe_fn)]
use super::*;
use std::{
    cell::RefCell,
    ffi::{CStr, CString, OsStr, c_char, c_void},
    panic::{AssertUnwindSafe, catch_unwind},
};

pub extern "C" fn link() {}
thread_local! { static ERROR: RefCell<CString> = RefCell::new(CString::default()); }
fn call<T>(failure: T, f: impl FnOnce() -> io::Result<T>) -> T {
    let result = catch_unwind(AssertUnwindSafe(f))
        .unwrap_or_else(|_| Err(io::Error::other("filesystem panic")));
    match result {
        Ok(v) => v,
        Err(e) => {
            ERROR.with(|v| {
                *v.borrow_mut() = CString::new(e.to_string().replace('\0', "?")).unwrap_or_default()
            });
            failure
        }
    }
}
unsafe fn path<'a>(p: *const c_char) -> io::Result<&'a Path> {
    if p.is_null() {
        return Err(io::Error::from(io::ErrorKind::InvalidInput));
    }
    let bytes = CStr::from_ptr(p).to_bytes();
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        Ok(Path::new(OsStr::from_bytes(bytes)))
    }
    #[cfg(not(unix))]
    {
        Ok(Path::new(
            std::str::from_utf8(bytes).map_err(io::Error::other)?,
        ))
    }
}
fn string(p: &OsStr) -> io::Result<*mut c_char> {
    #[cfg(unix)]
    let bytes = {
        use std::os::unix::ffi::OsStrExt;
        p.as_bytes()
    };
    #[cfg(not(unix))]
    let bytes = p
        .to_str()
        .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?
        .as_bytes();
    Ok(CString::new(bytes)?.into_raw())
}
unsafe fn buffer<'a>(p: *const c_void, len: usize) -> io::Result<&'a [u8]> {
    if len == 0 {
        return Ok(&[]);
    }
    if p.is_null() || len > isize::MAX as usize {
        return Err(io::Error::from(io::ErrorKind::InvalidInput));
    }
    Ok(std::slice::from_raw_parts(p.cast(), len))
}
unsafe fn output<'a>(p: *mut c_void, len: usize) -> io::Result<&'a mut [u8]> {
    if len == 0 {
        return Ok(&mut []);
    }
    if p.is_null() || len > isize::MAX as usize {
        return Err(io::Error::from(io::ErrorKind::InvalidInput));
    }
    Ok(std::slice::from_raw_parts_mut(p.cast(), len))
}
pub struct KrkrFile {
    stream: Box<dyn FileStream>,
}
pub struct KrkrDirectory {
    entries: Vec<(CString, Metadata)>,
}
#[unsafe(no_mangle)]
pub extern "C" fn krkr_fs_last_error() -> *const c_char {
    ERROR.with(|v| v.borrow().as_ptr())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_fs_open(p: *const c_char, mode: u32) -> *mut KrkrFile {
    call(std::ptr::null_mut(), || {
        let mode = match mode {
            0 => OpenMode::Read,
            1 => OpenMode::Write,
            2 => OpenMode::Append,
            3 => OpenMode::Update,
            _ => return Err(io::Error::from(io::ErrorKind::InvalidInput)),
        };
        Ok(Box::into_raw(Box::new(KrkrFile {
            stream: filesystem().open(path(p)?, mode)?,
        })))
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_fs_close(file: *mut KrkrFile) {
    if !file.is_null() {
        call((), || {
            drop(Box::from_raw(file));
            Ok(())
        });
    }
}
unsafe fn file<'a>(p: *mut KrkrFile) -> io::Result<&'a mut dyn FileStream> {
    Ok(p.as_mut()
        .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?
        .stream
        .as_mut())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_fs_read(p: *mut KrkrFile, dst: *mut c_void, len: usize) -> usize {
    call(usize::MAX, || file(p)?.read(output(dst, len)?))
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_fs_write(p: *mut KrkrFile, src: *const c_void, len: usize) -> usize {
    call(usize::MAX, || {
        file(p)?.write_all(buffer(src, len)?)?;
        Ok(len)
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_fs_seek(p: *mut KrkrFile, offset: i64, whence: u32) -> u64 {
    call(u64::MAX, || {
        let at = match whence {
            0 if offset >= 0 => SeekFrom::Start(offset as u64),
            1 => SeekFrom::Current(offset),
            2 => SeekFrom::End(offset),
            _ => return Err(io::Error::from(io::ErrorKind::InvalidInput)),
        };
        file(p)?.seek(at)
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_fs_size(p: *mut KrkrFile) -> u64 {
    call(u64::MAX, || file(p)?.size())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_fs_set_len(p: *mut KrkrFile, len: u64) -> i32 {
    call(-1, || {
        file(p)?.set_len(len)?;
        Ok(0)
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_fs_flush(p: *mut KrkrFile) -> i32 {
    call(-1, || {
        file(p)?.flush()?;
        Ok(0)
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_fs_metadata(p: *const c_char, out: *mut Metadata) -> i32 {
    call(-1, || {
        let out = out
            .as_mut()
            .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
        *out = filesystem().metadata(path(p)?)?;
        Ok(0)
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_fs_directory_open(p: *const c_char) -> *mut KrkrDirectory {
    call(std::ptr::null_mut(), || {
        let entries = filesystem()
            .read_dir(path(p)?)?
            .into_iter()
            .map(|e| {
                let s = string(&e.name)?;
                Ok((CString::from_raw(s), e.metadata))
            })
            .collect::<io::Result<_>>()?;
        Ok(Box::into_raw(Box::new(KrkrDirectory { entries })))
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_fs_directory_entry(
    p: *const KrkrDirectory,
    index: usize,
    out: *mut Metadata,
) -> *const c_char {
    call(std::ptr::null(), || {
        let dir = p
            .as_ref()
            .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
        match dir.entries.get(index) {
            Some((name, meta)) => {
                if let Some(out) = out.as_mut() {
                    *out = *meta;
                }
                Ok(name.as_ptr())
            }
            None => Ok(std::ptr::null()),
        }
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_fs_directory_close(p: *mut KrkrDirectory) {
    if !p.is_null() {
        call((), || {
            drop(Box::from_raw(p));
            Ok(())
        });
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_fs_create_dir(p: *const c_char, recursive: i32) -> i32 {
    call(-1, || {
        filesystem().create_dir(path(p)?, recursive != 0)?;
        Ok(0)
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_fs_remove(p: *const c_char, directory: i32) -> i32 {
    call(-1, || {
        let fs = filesystem();
        if directory != 0 {
            fs.remove_dir(path(p)?)?;
        } else {
            fs.remove_file(path(p)?)?;
        }
        Ok(0)
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_fs_copy(
    from: *const c_char,
    to: *const c_char,
    overwrite: i32,
) -> i32 {
    call(-1, || {
        filesystem().copy(path(from)?, path(to)?, overwrite != 0)?;
        Ok(0)
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_fs_rename(
    from: *const c_char,
    to: *const c_char,
    overwrite: i32,
) -> i32 {
    call(-1, || {
        filesystem().rename(path(from)?, path(to)?, overwrite != 0)?;
        Ok(0)
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_fs_replace(p: *const c_char, data: *const c_void, len: usize) -> i32 {
    call(-1, || {
        filesystem().replace(path(p)?, buffer(data, len)?)?;
        Ok(0)
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_fs_resolve(p: *const c_char, insensitive: i32) -> *mut c_char {
    call(std::ptr::null_mut(), || {
        string(
            filesystem()
                .resolve(path(p)?, insensitive != 0)?
                .as_os_str(),
        )
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn krkr_fs_writable_root() -> *mut c_char {
    call(std::ptr::null_mut(), || {
        string(filesystem().writable_root()?.as_os_str())
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_fs_string_free(p: *mut c_char) {
    if !p.is_null() {
        drop(CString::from_raw(p));
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_fs_set_modified(p: *const c_char, seconds: i64) -> i32 {
    call(-1, || {
        filesystem().set_modified(path(p)?, seconds)?;
        Ok(0)
    })
}
