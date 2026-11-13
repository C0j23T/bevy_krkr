#![cfg(target_os = "linux")]
use krkr_storage::{ffi::*, *};
use std::{
    ffi::{CStr, CString, OsString},
    io::{Read, Seek, SeekFrom, Write},
    os::unix::ffi::{OsStrExt, OsStringExt},
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!(
            "krkr-storage-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&p).unwrap();
        Self(p)
    }
    fn c(&self, name: &str) -> CString {
        CString::new(self.0.join(name).as_os_str().as_bytes()).unwrap()
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
#[test]
fn linux_streams_paths_and_mutations() {
    let temp = Temp::new();
    let fs = LinuxFileSystem;
    let source = temp.0.join("中文.TXT");
    fs.write(&source, b"abcdef").unwrap();
    let mut file = fs.open(&source, OpenMode::Update).unwrap();
    file.seek(SeekFrom::Start(2)).unwrap();
    file.write_all(b"XY").unwrap();
    file.set_len(4).unwrap();
    assert_eq!(file.size().unwrap(), 4);
    file.seek(SeekFrom::Start(0)).unwrap();
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, b"abXY");
    fs.open(&source, OpenMode::Append)
        .unwrap()
        .write_all(b"!")
        .unwrap();
    assert_eq!(fs.read(&source).unwrap(), b"abXY!");
    assert!(fs.open(&temp.0.join("missing"), OpenMode::Read).is_err());
    let link = temp.0.join("link");
    std::os::unix::fs::symlink(&source, &link).unwrap();
    assert_eq!(
        fs.read_dir(&temp.0)
            .unwrap()
            .into_iter()
            .find(|e| e.name == "link")
            .unwrap()
            .metadata
            .kind,
        FILE
    );
    let lower = temp.0.join("中文.txt");
    assert_eq!(fs.resolve(&lower, true).unwrap(), source);
    let odd = temp.0.join(OsString::from_vec(vec![b'x', 0xff]));
    fs.write(&odd, b"odd").unwrap();
    assert!(
        fs.read_dir(&temp.0)
            .unwrap()
            .iter()
            .any(|e| e.name == odd.file_name().unwrap())
    );
    let copy = temp.0.join("copy");
    fs.copy(&source, &copy, false).unwrap();
    assert!(fs.copy(&source, &copy, false).is_err());
    fs.write(&copy, b"old").unwrap();
    fs.copy(&source, &copy, true).unwrap();
    assert!(fs.rename(&source, &copy, false).is_err());
    assert!(source.exists());
    let moved = temp.0.join("moved");
    fs.rename(&copy, &moved, false).unwrap();
    assert!(!copy.exists());
    fs.replace(&source, b"replacement").unwrap();
    assert_eq!(fs.read(&source).unwrap(), b"replacement");
    assert!(fs.replace(&temp.0, b"bad").is_err());
    assert!(
        !fs.read_dir(&temp.0)
            .unwrap()
            .iter()
            .any(|e| e.name.to_string_lossy().starts_with(".krkr-"))
    );
    fs.set_modified(&source, 123456789).unwrap();
    assert_eq!(fs.metadata(&source).unwrap().modified, 123456789);
    let dir = temp.0.join("a/b");
    fs.create_dir(&dir, true).unwrap();
    assert!(fs.create_dir(&dir, false).is_err());
    assert!(fs.remove_dir(&temp.0.join("a")).is_err());
    fs.remove_dir(&dir).unwrap();
    let moved_dir = temp.0.join("moved-dir");
    fs.rename(&temp.0.join("a"), &moved_dir, false).unwrap();
    fs.remove_dir(&moved_dir).unwrap();
    fs.remove_file(&moved).unwrap();
}
#[test]
fn abi_owns_streams_snapshots_and_errors() {
    let temp = Temp::new();
    let path = temp.c("file");
    unsafe {
        assert!(krkr_fs_open(std::ptr::null(), 0).is_null());
        assert_eq!(
            krkr_fs_read(std::ptr::null_mut(), std::ptr::null_mut(), 0),
            usize::MAX
        );
        assert!(!CStr::from_ptr(krkr_fs_last_error()).to_bytes().is_empty());
        let f = krkr_fs_open(path.as_ptr(), 1);
        assert!(!f.is_null());
        assert_eq!(krkr_fs_write(f, b"abcdef".as_ptr().cast(), 6), 6);
        assert_eq!(krkr_fs_seek(f, -2, 2), 4);
        assert_eq!(krkr_fs_set_len(f, 4), 0);
        assert_eq!(krkr_fs_size(f), 4);
        assert_eq!(krkr_fs_seek(f, -1, 0), u64::MAX);
        assert_eq!(krkr_fs_read(f, std::ptr::null_mut(), 1), usize::MAX);
        assert_eq!(krkr_fs_read(f, std::ptr::null_mut(), 0), 0);
        krkr_fs_seek(f, 0, 0);
        let mut bytes = [0; 8];
        assert_eq!(krkr_fs_read(f, bytes.as_mut_ptr().cast(), 8), 4);
        assert_eq!(&bytes[..4], b"abcd");
        assert_eq!(krkr_fs_read(f, bytes.as_mut_ptr().cast(), 8), 0);
        krkr_fs_close(f);
        let directory = temp.c("");
        let d = krkr_fs_directory_open(directory.as_ptr());
        assert!(!d.is_null());
        let mut meta = Metadata::default();
        let name = krkr_fs_directory_entry(d, 0, &mut meta);
        assert_eq!(CStr::from_ptr(name).to_bytes(), b"file");
        assert_eq!(meta.size, 4);
        assert!(krkr_fs_directory_entry(d, 1, &mut meta).is_null());
        krkr_fs_directory_close(d);
        let resolved = krkr_fs_resolve(path.as_ptr(), 0);
        assert!(!resolved.is_null());
        krkr_fs_string_free(resolved);
        assert_eq!(krkr_fs_replace(path.as_ptr(), std::ptr::null(), 1), -1);
        assert_eq!(krkr_fs_replace(path.as_ptr(), std::ptr::null(), 0), 0);
    }
}
#[test]
fn dummy_fails_explicitly() {
    let dummy = DummyFileSystem;
    assert_eq!(
        dummy
            .write(std::path::Path::new("unused"), b"x")
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::Unsupported
    );
    assert!(dummy.read_dir(std::path::Path::new("unused")).is_err());
}
