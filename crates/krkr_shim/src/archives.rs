//! 7z / ZIP reading, as a plain Rust API.

use std::io::{Read, Seek};
use std::sync::{Mutex, MutexGuard};

use sevenz_rust2::{Archive as SevenZArchive, ArchiveReader, Password};
use zip::ZipArchive;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    SevenZ,
    Zip,
}

#[derive(Debug, Clone)]
pub struct Entry {
    name: String,
    size: u64,
    format_index: usize,
}

impl Entry {
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Uncompressed size in bytes.
    pub fn size(&self) -> u64 {
        self.size
    }
}

#[allow(clippy::large_enum_variant)]
enum Reader<R: Read + Seek> {
    SevenZ(ArchiveReader<R>),
    Zip(ZipArchive<R>),
}

pub struct Archive<R: Read + Seek> {
    entries: Vec<Entry>,
    reader: Mutex<Reader<R>>,
}

impl<R: Read + Seek> Archive<R> {
    pub fn open(kind: Kind, source: R) -> Option<Self> {
        match kind {
            Kind::SevenZ => Self::open_7z(source),
            Kind::Zip => Self::open_zip(source),
        }
    }

    fn open_7z(mut source: R) -> Option<Self> {
        let archive = SevenZArchive::read(&mut source, &Password::empty()).ok()?;
        let entries: Vec<Entry> = archive
            .files
            .iter()
            .enumerate()
            .filter(|(_, file)| !file.is_directory && file.has_stream)
            .map(|(format_index, file)| Entry {
                name: file.name.clone(),
                size: file.size,
                format_index,
            })
            .collect();
        if entries.is_empty() {
            return None;
        }
        Some(Archive {
            entries,
            reader: Mutex::new(Reader::SevenZ(ArchiveReader::from_archive(
                archive,
                source,
                Password::empty(),
            ))),
        })
    }

    fn open_zip(source: R) -> Option<Self> {
        let mut zip = ZipArchive::new(source).ok()?;
        let mut entries = Vec::with_capacity(zip.len());
        for index in 0..zip.len() {
            let Ok(file) = zip.by_index_raw(index) else { continue };
            if file.is_dir() {
                continue;
            }
            let name = match file.name() {
                Ok(name) => name.into_owned(),
                Err(_) => String::from_utf8_lossy(file.name_raw()).into_owned(),
            };
            entries.push(Entry { name, size: file.size(), format_index: index });
        }
        if entries.is_empty() {
            return None;
        }
        Some(Archive { entries, reader: Mutex::new(Reader::Zip(zip)) })
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn read(&self, index: usize, out: &mut [u8]) -> Option<usize> {
        let entry = self.entries.get(index)?;
        if entry.size > out.len() as u64 {
            return None;
        }
        let name = entry.name.clone();
        let format_index = entry.format_index;
        let size = entry.size as usize;
        match &mut *self.lock() {
            Reader::SevenZ(sevenz) => {
                let data = sevenz.read_file(&name).ok()?;
                if data.len() > out.len() {
                    return None;
                }
                out[..data.len()].copy_from_slice(&data);
                Some(data.len())
            }
            Reader::Zip(zip) => {
                let mut file = zip.by_index(format_index).ok()?;
                let mut written = 0;
                while written < size {
                    match file.read(&mut out[written..size]) {
                        Ok(0) => break,
                        Ok(n) => written += n,
                        Err(_) => return None,
                    }
                }
                (written == size).then_some(written)
            }
        }
    }

    fn lock(&self) -> MutexGuard<'_, Reader<R>> {
        self.reader.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// Real archives: `sample.7z` (LZMA2, what games ship) and `sample.zip`
    /// (deflate), each with a root file, a nested one, one with a Japanese
    /// name, and directory entries that are not files.
    const SEVEN_Z: &[u8] = include_bytes!("../tests/data/sample.7z");
    const ZIP: &[u8] = include_bytes!("../tests/data/sample.zip");

    /// `(name, contents)` of everything the archive holds.
    fn expected() -> Vec<(&'static str, &'static [u8])> {
        vec![
            ("hello.txt", b"hello, 7z\n"),
            ("日本語/セリフ.txt", "セリフです\n".as_bytes()),
            ("sub/deep.txt", b"nested\n"),
        ]
    }

    fn open(kind: Kind, data: &'static [u8]) -> Archive<Cursor<&'static [u8]>> {
        Archive::open(kind, Cursor::new(data)).expect("the fixture opens")
    }

    /// Directories are not entries, and names come back as UTF-8.
    fn check_listing(kind: Kind, data: &'static [u8]) {
        let mut listed: Vec<String> =
            open(kind, data).entries().iter().map(|e| e.name().to_string()).collect();
        listed.sort();
        let mut want: Vec<String> = expected().iter().map(|(name, _)| name.to_string()).collect();
        want.sort();
        assert_eq!(listed, want);
    }

    #[test]
    fn seven_z_lists_its_files() {
        check_listing(Kind::SevenZ, SEVEN_Z);
    }

    #[test]
    fn zip_lists_its_files() {
        check_listing(Kind::Zip, ZIP);
    }

    /// Every entry decompresses to exactly its declared size, with the right
    /// bytes: the caller allocates from `Entry::size`, so a mismatch would hand
    /// the engine a corrupt stream.
    fn check_contents(kind: Kind, data: &'static [u8]) {
        let archive = open(kind, data);
        for (name, want) in expected() {
            let index = archive
                .entries()
                .iter()
                .position(|entry| entry.name() == name)
                .unwrap_or_else(|| panic!("{name} missing"));
            let mut buf = vec![0u8; archive.entries()[index].size() as usize];
            let written = archive.read(index, &mut buf).expect("the entry decodes");
            assert_eq!(written, buf.len(), "{name} decoded to the wrong size");
            assert_eq!(buf, want, "{name} has the wrong contents");
        }
    }

    #[test]
    fn seven_z_reads_every_entry() {
        check_contents(Kind::SevenZ, SEVEN_Z);
    }

    #[test]
    fn zip_reads_every_entry() {
        check_contents(Kind::Zip, ZIP);
    }

    /// Both formats are recognised by their header, so the wrong `Kind` is
    /// rejected instead of producing an empty or half-parsed archive.
    #[test]
    fn each_kind_rejects_the_other_format() {
        assert!(Archive::open(Kind::Zip, Cursor::new(SEVEN_Z)).is_none());
        assert!(Archive::open(Kind::SevenZ, Cursor::new(ZIP)).is_none());
    }

    /// Out-of-range indices and buffers too small for the entry report failure
    /// rather than reading past the entry list or copying a prefix.
    #[test]
    fn bad_reads_are_refused() {
        let archive = open(Kind::SevenZ, SEVEN_Z);
        let count = archive.entries().len();
        let mut buf = [0u8; 32];
        assert_eq!(archive.read(count, &mut buf), None, "index past the end");

        let index = archive
            .entries()
            .iter()
            .position(|entry| entry.size() == 10)
            .expect("hello.txt is 10 bytes");
        let mut small = [0u8; 4];
        assert_eq!(archive.read(index, &mut small), None, "buffer smaller than the entry");
    }

    /// A zero-length buffer for a zero-length entry is a valid, empty read; the
    /// engine asks for those (empty script files, marker entries).
    #[test]
    fn an_empty_buffer_reads_nothing() {
        let archive = open(Kind::Zip, ZIP);
        // None of the fixtures is empty, so this only checks the guard: a
        // non-empty entry must not be read into nothing.
        assert_eq!(archive.read(0, &mut []), None);
    }
}
