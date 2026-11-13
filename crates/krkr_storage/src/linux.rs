use super::*;
use std::{
    fs::{self, OpenOptions},
    os::unix::{ffi::OsStrExt, fs::MetadataExt},
    sync::atomic::{AtomicU64, Ordering},
};

pub struct LinuxFileSystem;
fn metadata(m: fs::Metadata) -> Metadata {
    Metadata {
        kind: if m.is_file() {
            FILE
        } else if m.is_dir() {
            DIRECTORY
        } else {
            0
        },
        size: m.len(),
        accessed: m.atime(),
        modified: m.mtime(),
        changed: m.ctime(),
    }
}
impl FileSystem for LinuxFileSystem {
    fn open(&self, path: &Path, mode: OpenMode) -> io::Result<Box<dyn FileStream>> {
        let mut options = OpenOptions::new();
        match mode {
            OpenMode::Read => {
                options.read(true);
            }
            OpenMode::Write => {
                options.read(true).write(true).create(true).truncate(true);
            }
            OpenMode::Append => {
                options.read(true).append(true).create(true);
            }
            OpenMode::Update => {
                options.read(true).write(true);
            }
        }
        let mut file = options.open(path)?;
        if matches!(mode, OpenMode::Append) {
            file.seek(SeekFrom::End(0))?;
        }
        Ok(Box::new(file))
    }
    fn metadata(&self, path: &Path) -> io::Result<Metadata> {
        Ok(metadata(fs::metadata(path)?))
    }
    fn read_dir(&self, path: &Path) -> io::Result<Vec<Entry>> {
        fs::read_dir(path)?
            .map(|entry| {
                let e = entry?;
                Ok(Entry {
                    name: e.file_name(),
                    metadata: metadata(fs::metadata(e.path())?),
                })
            })
            .collect()
    }
    fn create_dir(&self, path: &Path, recursive: bool) -> io::Result<()> {
        if recursive {
            fs::create_dir_all(path)
        } else {
            fs::create_dir(path)
        }
    }
    fn remove_file(&self, path: &Path) -> io::Result<()> {
        fs::remove_file(path)
    }
    fn remove_dir(&self, path: &Path) -> io::Result<()> {
        fs::remove_dir(path)
    }
    fn copy(&self, from: &Path, to: &Path, overwrite: bool) -> io::Result<()> {
        if overwrite {
            fs::copy(from, to)?;
        } else {
            let mut src = fs::File::open(from)?;
            let mut dst = OpenOptions::new().write(true).create_new(true).open(to)?;
            io::copy(&mut src, &mut dst)?;
        }
        Ok(())
    }
    fn rename(&self, from: &Path, to: &Path, overwrite: bool) -> io::Result<()> {
        if overwrite {
            return fs::rename(from, to);
        }
        let source = std::ffi::CString::new(from.as_os_str().as_bytes())?;
        let destination = std::ffi::CString::new(to.as_os_str().as_bytes())?;
        // Both strings remain live and NUL-terminated throughout the syscall.
        let result = unsafe {
            libc::renameat2(
                libc::AT_FDCWD,
                source.as_ptr(),
                libc::AT_FDCWD,
                destination.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        };
        if result != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    fn replace(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let (temp, mut file) = loop {
            let temp = parent.join(format!(
                ".krkr-{}-{}.tmp",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match OpenOptions::new().write(true).create_new(true).open(&temp) {
                Ok(file) => break (temp, file),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e),
            }
        };
        let result = (|| {
            file.write_all(bytes)?;
            file.flush()?;
            drop(file);
            fs::rename(&temp, path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(temp);
        }
        result
    }
    fn resolve(&self, path: &Path, case_insensitive: bool) -> io::Result<PathBuf> {
        let absolute = std::path::absolute(path)?;
        let mut result = PathBuf::new();
        for component in absolute.components() {
            use std::path::Component;
            match component {
                Component::ParentDir => {
                    if let Ok(real) = fs::canonicalize(&result) {
                        result = real;
                    }
                    result.pop();
                }
                Component::CurDir => {}
                Component::Normal(name) if case_insensitive => {
                    let mut entries = fs::read_dir(&result)
                        .ok()
                        .into_iter()
                        .flatten()
                        .filter_map(Result::ok)
                        .collect::<Vec<_>>();
                    entries.sort_by_key(|e| e.file_name());
                    let exact = entries.iter().find(|e| e.file_name() == name);
                    let found = exact.or_else(|| {
                        entries.iter().find(|e| {
                            e.file_name()
                                .as_bytes()
                                .eq_ignore_ascii_case(name.as_bytes())
                        })
                    });
                    result.push(
                        found
                            .map(|e| e.file_name())
                            .unwrap_or_else(|| name.to_os_string()),
                    );
                }
                c => result.push(c.as_os_str()),
            }
        }
        Ok(result)
    }
    fn set_modified(&self, path: &Path, seconds: i64) -> io::Result<()> {
        let time = if seconds >= 0 {
            std::time::UNIX_EPOCH.checked_add(std::time::Duration::from_secs(seconds as u64))
        } else {
            std::time::UNIX_EPOCH
                .checked_sub(std::time::Duration::from_secs(seconds.unsigned_abs()))
        }
        .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
        fs::File::open(path)?.set_times(fs::FileTimes::new().set_modified(time))
    }
    fn writable_root(&self) -> io::Result<PathBuf> {
        let base = std::env::var_os("XDG_DATA_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME")
                    .filter(|v| !v.is_empty())
                    .map(|v| PathBuf::from(v).join(".local/share"))
            })
            .unwrap_or_else(std::env::temp_dir)
            .join("bevy_krkr");
        fs::create_dir_all(&base)?;
        Ok(base)
    }
}
