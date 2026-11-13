use std::ffi::{c_char, c_void};
use std::path::PathBuf;

use crate::xml;

use super::{c_string, guard, input};

/// `KRKR_PREFERENCE_*` in `krkr_shim.h`.
pub const KRKR_PREFERENCE_ITEM: i32 = 0;
pub const KRKR_PREFERENCE_CUSTOM: i32 = 1;
pub const KRKR_PREFERENCE_KEYMAP: i32 = 2;

/// # Safety
/// `ptr` must be null or point at a NUL-terminated string.
unsafe fn text<'a>(ptr: *const c_char) -> Option<&'a str> {
    if ptr.is_null() {
        return None;
    }
    std::ffi::CStr::from_ptr(ptr).to_str().ok()
}

/// A path is bytes on Unix — the engine's directories can hold names that are
/// not UTF-8 — so it does not go through [`text`].
///
/// # Safety
/// `ptr` must be null or point at a NUL-terminated string.
unsafe fn path(ptr: *const c_char) -> Option<PathBuf> {
    if ptr.is_null() {
        return None;
    }
    let bytes = std::ffi::CStr::from_ptr(ptr).to_bytes();
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        Some(std::path::Path::new(std::ffi::OsStr::from_bytes(bytes)).to_path_buf())
    }
    #[cfg(not(unix))]
    {
        Some(PathBuf::from(std::str::from_utf8(bytes).ok()?))
    }
}

/// Callbacks for [`krkr_preference_read`]. They run on this crate's stack, so a
/// C++ exception escaping one aborts the process; they must not call back into
/// the shim either. Both strings are UTF-8 and NUL-terminated, and live only
/// for the call.
#[repr(C)]
pub struct KrkrPreferenceVisitor {
    pub userdata: *mut c_void,
    /// A setting or an extra command-line argument, `list` being
    /// `KRKR_PREFERENCE_ITEM` or `KRKR_PREFERENCE_CUSTOM`.
    pub entry: Option<unsafe extern "C" fn(*mut c_void, i32, *const c_char, *const c_char)>,
    /// A key map entry, already read as numbers.
    pub keymap: Option<unsafe extern "C" fn(*mut c_void, i32, i32)>,
}

/// Callback for [`krkr_locale_read`]; same rules as [`KrkrPreferenceVisitor`].
#[repr(C)]
pub struct KrkrTextVisitor {
    pub userdata: *mut c_void,
    pub text: Option<unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char)>,
}

/// One record for [`krkr_preference_write`].
#[repr(C)]
pub struct KrkrPreferenceRecord {
    /// `KRKR_PREFERENCE_ITEM`, `KRKR_PREFERENCE_CUSTOM` or `KRKR_PREFERENCE_KEYMAP`.
    pub list: i32,
    /// Key map entries only: the key code before and after the remap.
    pub key: i32,
    pub value: i32,
    /// Settings and arguments only: UTF-8, NUL-terminated, read during the call.
    pub key_text: *const c_char,
    pub value_text: *const c_char,
}

/// 0 when the file was read and is well-formed XML, -1 otherwise.
///
/// # Safety
/// `path_ptr` must be NUL-terminated; `visitor` must be readable and its
/// callbacks must obey [`KrkrPreferenceVisitor`]'s rules.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_preference_read(
    path_ptr: *const c_char,
    visitor: *const KrkrPreferenceVisitor,
) -> i32 {
    guard(-1, || {
        let (Some(path), Some(visitor)) = (path(path_ptr), visitor.as_ref()) else {
            return -1;
        };
        let Ok(xml) = krkr_storage::filesystem().read(&path) else {
            return -1;
        };
        let Some(preference) = xml::Preference::parse(&xml) else {
            return -1;
        };

        if let Some(entry) = visitor.entry {
            let settings = preference
                .items
                .iter()
                .map(|(key, value)| (KRKR_PREFERENCE_ITEM, key, value))
                .chain(
                    preference
                        .custom_arguments
                        .iter()
                        .map(|(key, value)| (KRKR_PREFERENCE_CUSTOM, key, value)),
                );
            for (list, key, value) in settings {
                let (key, value) = (c_string(key), c_string(value));
                entry(visitor.userdata, list, key.as_ptr(), value.as_ptr());
            }
        }
        if let Some(keymap) = visitor.keymap {
            for &(key, value) in &preference.keymap {
                keymap(visitor.userdata, key, value);
            }
        }
        0
    })
}

/// Writes the records in the order given, which is the order they are read
/// back in. 0 when the file was written, -1 when it could not be.
///
/// # Safety
/// `path_ptr` must be NUL-terminated; `records` must hold `count` readable
/// records whose strings are NUL-terminated (or be null when `count` is 0).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_preference_write(
    path_ptr: *const c_char,
    records: *const KrkrPreferenceRecord,
    count: usize,
) -> i32 {
    guard(-1, || {
        let Some(path) = path(path_ptr) else {
            return -1;
        };
        let records: &[KrkrPreferenceRecord] = if records.is_null() || count == 0 {
            &[]
        } else {
            std::slice::from_raw_parts(records, count)
        };
        let mut preference = xml::Preference::default();
        let record_text = |ptr: *const c_char| text(ptr).unwrap_or_default().to_owned();
        for record in records {
            match record.list {
                KRKR_PREFERENCE_ITEM => preference
                    .items
                    .push((record_text(record.key_text), record_text(record.value_text))),
                KRKR_PREFERENCE_CUSTOM => preference
                    .custom_arguments
                    .push((record_text(record.key_text), record_text(record.value_text))),
                KRKR_PREFERENCE_KEYMAP => preference.keymap.push((record.key, record.value)),
                _ => {}
            }
        }
        match krkr_storage::filesystem().write(&path, preference.to_xml().as_bytes()) {
            Ok(()) => 0,
            Err(_) => -1,
        }
    })
}

/// The message count; a document that is not well-formed XML has none.
///
/// The engine reads the file itself — it can live inside an archive — so the
/// contents come in rather than a path.
///
/// # Safety
/// `data` must hold `len` bytes (or be null if `len` is 0); `visitor` must be
/// readable and its callback must obey [`KrkrTextVisitor`]'s rules.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_locale_read(
    data: *const c_void,
    len: usize,
    visitor: *const KrkrTextVisitor,
) -> usize {
    guard(0, || {
        let Some(visitor) = visitor.as_ref() else {
            return 0;
        };
        let Some(text_callback) = visitor.text else {
            return 0;
        };
        let Some(locale) = xml::Locale::parse(input(data, len)) else {
            return 0;
        };
        for (id, text) in &locale.texts {
            let (id, text) = (c_string(id), c_string(text));
            text_callback(visitor.userdata, id.as_ptr(), text.as_ptr());
        }
        locale.texts.len()
    })
}
