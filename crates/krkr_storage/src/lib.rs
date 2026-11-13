//! Synchronous host filesystem used by the Kirikiri Storage adapter.
use std::{
    ffi::OsString,
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{Arc, LazyLock, RwLock},
};

pub mod ffi;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::LinuxFileSystem;

#[derive(Clone, Copy, Debug)]
pub enum OpenMode {
    Read,
    Write,
    Append,
    Update,
}
#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct Metadata {
    pub kind: u32,
    pub size: u64,
    pub accessed: i64,
    pub modified: i64,
    pub changed: i64,
}
pub const FILE: u32 = 1;
pub const DIRECTORY: u32 = 2;
pub struct Entry {
    pub name: OsString,
    pub metadata: Metadata,
}
pub trait FileStream: Read + Write + Seek + Send {
    fn size(&self) -> io::Result<u64>;
    fn set_len(&mut self, len: u64) -> io::Result<()>;
}
impl FileStream for std::fs::File {
    fn size(&self) -> io::Result<u64> {
        Ok(self.metadata()?.len())
    }
    fn set_len(&mut self, len: u64) -> io::Result<()> {
        std::fs::File::set_len(self, len)
    }
}
fn unsupported<T>() -> io::Result<T> {
    Err(io::Error::from(io::ErrorKind::Unsupported))
}
pub trait FileSystem: Send + Sync {
    fn open(&self, _path: &Path, _mode: OpenMode) -> io::Result<Box<dyn FileStream>> {
        unsupported()
    }
    fn metadata(&self, _path: &Path) -> io::Result<Metadata> {
        unsupported()
    }
    fn read_dir(&self, _path: &Path) -> io::Result<Vec<Entry>> {
        unsupported()
    }
    fn create_dir(&self, _path: &Path, _recursive: bool) -> io::Result<()> {
        unsupported()
    }
    fn remove_file(&self, _path: &Path) -> io::Result<()> {
        unsupported()
    }
    fn remove_dir(&self, _path: &Path) -> io::Result<()> {
        unsupported()
    }
    fn copy(&self, _from: &Path, _to: &Path, _overwrite: bool) -> io::Result<()> {
        unsupported()
    }
    fn rename(&self, _from: &Path, _to: &Path, _overwrite: bool) -> io::Result<()> {
        unsupported()
    }
    fn replace(&self, _path: &Path, _bytes: &[u8]) -> io::Result<()> {
        unsupported()
    }
    fn resolve(&self, _path: &Path, _case_insensitive: bool) -> io::Result<PathBuf> {
        unsupported()
    }
    fn set_modified(&self, _path: &Path, _seconds: i64) -> io::Result<()> {
        unsupported()
    }
    fn writable_root(&self) -> io::Result<PathBuf> {
        unsupported()
    }
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        let mut bytes = Vec::new();
        self.open(path, OpenMode::Read)?.read_to_end(&mut bytes)?;
        Ok(bytes)
    }
    fn write(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        let mut file = self.open(path, OpenMode::Write)?;
        file.write_all(bytes)?;
        file.flush()
    }
}
pub struct DummyFileSystem;
impl FileSystem for DummyFileSystem {}
static BACKEND: LazyLock<RwLock<Arc<dyn FileSystem>>> = LazyLock::new(|| {
    #[cfg(target_os = "linux")]
    let backend: Arc<dyn FileSystem> = Arc::new(LinuxFileSystem);
    #[cfg(not(target_os = "linux"))]
    let backend: Arc<dyn FileSystem> = Arc::new(DummyFileSystem);
    RwLock::new(backend)
});
pub fn filesystem() -> Arc<dyn FileSystem> {
    BACKEND.read().unwrap_or_else(|e| e.into_inner()).clone()
}
/// Install before starting the engine. Existing streams keep their current backend.
pub fn set_filesystem(fs: Arc<dyn FileSystem>) {
    *BACKEND.write().unwrap_or_else(|e| e.into_inner()) = fs;
}
