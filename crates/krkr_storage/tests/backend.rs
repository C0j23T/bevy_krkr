#![cfg(target_os = "linux")]
use krkr_storage::{ffi::*, *};
use std::{ffi::CString, sync::Arc};
#[test]
fn backend_switch_preserves_open_stream() {
    let path = std::env::temp_dir().join(format!("krkr-backend-{}", std::process::id()));
    let name = CString::new(path.to_str().unwrap()).unwrap();
    unsafe {
        let file = krkr_fs_open(name.as_ptr(), 1);
        assert!(!file.is_null());
        set_filesystem(Arc::new(DummyFileSystem));
        assert!(krkr_fs_open(name.as_ptr(), 0).is_null());
        assert_eq!(krkr_fs_write(file, b"ok".as_ptr().cast(), 2), 2);
        krkr_fs_close(file);
        set_filesystem(Arc::new(LinuxFileSystem));
    }
    assert_eq!(std::fs::read(&path).unwrap(), b"ok");
    std::fs::remove_file(path).unwrap();
}
