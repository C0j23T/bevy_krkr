#![allow(unsafe_op_in_unsafe_fn)]

//! The preference and locale files through the C ABI, the way
//! `environ/ConfigManager` drives them: real files, visitors called from Rust,
//! the sentinels. The format itself is tested in `src/xml.rs`.

use std::collections::BTreeMap;
use std::ffi::{c_char, c_void, CStr, CString};
use std::path::PathBuf;

use krkr_shim::ffi::{
    krkr_locale_read, krkr_preference_read, krkr_preference_write, KrkrPreferenceRecord,
    KrkrPreferenceVisitor, KrkrTextVisitor, KRKR_PREFERENCE_CUSTOM, KRKR_PREFERENCE_ITEM,
    KRKR_PREFERENCE_KEYMAP,
};

/// A file in the temp directory, removed on drop: the entry points take a path.
struct TempFile(PathBuf);

impl TempFile {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("krkr_shim_{}_{name}.xml", std::process::id()));
        let _ = std::fs::remove_file(&path);
        TempFile(path)
    }

    fn c_path(&self) -> CString {
        CString::new(self.0.to_str().expect("a UTF-8 temp path")).expect("no interior NUL")
    }

    fn write(&self, contents: &str) {
        std::fs::write(&self.0, contents).expect("writes the fixture");
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// What `Config::Entries` collects on the C++ side.
#[derive(Debug, Default, PartialEq)]
struct Collected {
    items: Vec<(String, String)>,
    custom: Vec<(String, String)>,
    keymap: Vec<(i32, i32)>,
}

unsafe fn string(ptr: *const c_char) -> String {
    CStr::from_ptr(ptr).to_str().expect("UTF-8 out").to_owned()
}

unsafe extern "C" fn collect_entry(
    userdata: *mut c_void,
    list: i32,
    key: *const c_char,
    value: *const c_char,
) {
    let collected = &mut *(userdata as *mut Collected);
    let entry = (string(key), string(value));
    match list {
        KRKR_PREFERENCE_ITEM => collected.items.push(entry),
        KRKR_PREFERENCE_CUSTOM => collected.custom.push(entry),
        _ => {}
    }
}

unsafe extern "C" fn collect_keymap(userdata: *mut c_void, key: i32, value: i32) {
    (*(userdata as *mut Collected)).keymap.push((key, value));
}

/// Reads `path` through the C ABI; the status is the entry point's return
/// value, -1 being "not a readable preference file".
fn read(path: &TempFile) -> (i32, Collected) {
    let mut collected = Collected::default();
    let visitor = KrkrPreferenceVisitor {
        userdata: (&mut collected as *mut Collected).cast(),
        entry: Some(collect_entry),
        keymap: Some(collect_keymap),
    };
    let path = path.c_path();
    let status = unsafe { krkr_preference_read(path.as_ptr(), &visitor) };
    (status, collected)
}

/// Writes records the way `Config::SavePreference` lays them out.
fn write(path: &TempFile, collected: &Collected) -> i32 {
    // The records only borrow their text: moving a `CString` moves the handle,
    // not the bytes, so these pointers stay valid however `texts` grows.
    let mut texts: Vec<CString> = Vec::new();
    let mut records: Vec<KrkrPreferenceRecord> = Vec::new();
    let mut add = |list: i32, key: &str, value: &str, texts: &mut Vec<CString>| {
        let (key, value) = (
            CString::new(key).expect("no interior NUL"),
            CString::new(value).expect("no interior NUL"),
        );
        texts.push(key);
        texts.push(value);
        records.push(KrkrPreferenceRecord {
            list,
            key: 0,
            value: 0,
            key_text: texts[texts.len() - 2].as_ptr(),
            value_text: texts[texts.len() - 1].as_ptr(),
        });
    };
    for (key, value) in &collected.items {
        add(KRKR_PREFERENCE_ITEM, key, value, &mut texts);
    }
    for (key, value) in &collected.custom {
        add(KRKR_PREFERENCE_CUSTOM, key, value, &mut texts);
    }
    for &(key, value) in &collected.keymap {
        records.push(KrkrPreferenceRecord {
            list: KRKR_PREFERENCE_KEYMAP,
            key,
            value,
            key_text: std::ptr::null(),
            value_text: std::ptr::null(),
        });
    }
    let path = path.c_path();
    unsafe { krkr_preference_write(path.as_ptr(), records.as_ptr(), records.len()) }
}

#[test]
fn writes_a_file_the_engine_reads_back() {
    let file = TempFile::new("roundtrip");
    let written = Collected {
        items: vec![
            (String::from("user_language"), String::from("jp")),
            (String::from("path"), String::from("C:\\games & such")),
        ],
        custom: vec![(String::from("window"), String::from("normal"))],
        keymap: vec![(13, 27)],
    };
    assert_eq!(write(&file, &written), 0);

    let (status, read_back) = read(&file);
    assert_eq!(status, 0);
    assert_eq!(read_back, written, "entities and order survive the round trip");
}

/// A game started for the first time has no file yet, and a failed save leaves
/// garbage; both mean "no settings".
#[test]
fn a_missing_or_broken_file_reports_failure_and_no_records() {
    let missing = TempFile::new("missing");
    assert_eq!(read(&missing), (-1, Collected::default()));

    let broken = TempFile::new("broken");
    broken.write("<GlobalPreference><Item key=\"a\"></GlobalPreference>");
    assert_eq!(read(&broken), (-1, Collected::default()));
}

#[test]
fn a_visitor_without_a_keymap_callback_gets_only_the_settings() {
    let file = TempFile::new("partial");
    file.write(
        "<GlobalPreference><Item key=\"a\" value=\"1\"/><KeyMap key=\"13\" value=\"27\"/>\
         </GlobalPreference>",
    );
    let mut collected = Collected::default();
    let visitor = KrkrPreferenceVisitor {
        userdata: (&mut collected as *mut Collected).cast(),
        entry: Some(collect_entry),
        keymap: None,
    };
    let path = file.c_path();
    assert_eq!(unsafe { krkr_preference_read(path.as_ptr(), &visitor) }, 0);
    assert_eq!(collected.items, vec![(String::from("a"), String::from("1"))]);
    assert_eq!(collected.keymap, vec![]);
}

/// This is what makes the caller show its "read-only storage" message.
#[test]
fn saving_where_it_cannot_write_reports_failure() {
    let mut path = std::env::temp_dir();
    path.push(format!("krkr_shim_{}_no_such_dir", std::process::id()));
    path.push("GlobalPreference.xml");
    let path = CString::new(path.to_str().expect("a UTF-8 temp path")).expect("no interior NUL");
    assert_eq!(unsafe { krkr_preference_write(path.as_ptr(), std::ptr::null(), 0) }, -1);
}

unsafe extern "C" fn collect_text(userdata: *mut c_void, id: *const c_char, text: *const c_char) {
    (*(userdata as *mut BTreeMap<String, String>)).insert(string(id), string(text));
}

fn read_locale(xml: &str) -> (usize, BTreeMap<String, String>) {
    let mut texts = BTreeMap::new();
    let visitor = KrkrTextVisitor {
        userdata: (&mut texts as *mut BTreeMap<String, String>).cast(),
        text: Some(collect_text),
    };
    let count = unsafe { krkr_locale_read(xml.as_ptr().cast(), xml.len(), &visitor) };
    (count, texts)
}

#[test]
fn reads_the_locale_table() {
    let xml = "<Locale>\
        <Text id=\"ok\" text=\"OK\"/>\
        <Text id=\"cancel\" text=\"Cancel &amp; back\"/>\
        <Text id=\"broken\"/>\
        <Group id=\"group\" text=\"whatever\"/>\
        </Locale>";
    let (count, texts) = read_locale(xml);
    assert_eq!(count, 3);
    assert_eq!(texts[&String::from("ok")], "OK");
    assert_eq!(texts[&String::from("cancel")], "Cancel & back");
    assert_eq!(texts[&String::from("group")], "whatever");
}

#[test]
fn a_file_that_is_not_xml_is_no_locale() {
    assert_eq!(read_locale("not xml at all"), (0, BTreeMap::new()));
    assert_eq!(read_locale(""), (0, BTreeMap::new()));
}
