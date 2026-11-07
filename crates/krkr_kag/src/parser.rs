//! Safe wrapper over the Kirikiroid2 KAG3 parser.
//!
//! Tag parsing happens in C++. The parser reports each tag through a C
//! callback; the callback only copies the tag into a thread-local queue, and
//! [`KagParser::step`] drains that queue after each `krkr_kag_process_next`.
//! No Bevy state is touched from inside the callback.

use std::cell::RefCell;
use std::ffi::{CStr, CString, NulError};
use std::os::raw::{c_char, c_void};

use krkr_script::Tjs2Engine;
use krkr_sys as sys;

/// One tag as reported by Kirikiroid2. Attribute values are strings.
#[derive(Debug, Clone, PartialEq)]
pub struct RawTag {
    pub name: String,
    pub attrs: serde_json::Map<String, serde_json::Value>,
}

impl RawTag {
    /// Build a tag from a name and `(key, value)` pairs. Mainly for tests and
    /// for game code that injects tags.
    pub fn new(name: &str, attrs: &[(&str, &str)]) -> Self {
        Self {
            name: name.to_owned(),
            attrs: attrs
                .iter()
                .map(|(k, v)| ((*k).to_owned(), serde_json::Value::String((*v).to_owned())))
                .collect(),
        }
    }
}

/// Result of one parser step.
#[derive(Debug)]
pub enum Step {
    /// Tags produced by this step (usually exactly one).
    Tags(Vec<RawTag>),
    /// The scenario has no more tags.
    End,
}

#[derive(Debug, thiserror::Error)]
pub enum KagError {
    #[error("krkr_kag_create failed: {0}")]
    CreateFailed(String),
    #[error("KAG {what} failed: {message}")]
    Ffi { what: &'static str, message: String },
    #[error("string contains a NUL byte: {0}")]
    InvalidString(#[from] NulError),
}

thread_local! {
    static PENDING_TAGS: RefCell<Vec<RawTag>> = const { RefCell::new(Vec::new()) };
}

/// Installed with `krkr_kag_set_tag_callback`. Copies the tag and returns.
unsafe extern "C" fn tag_callback(name: *const c_char, attrs_json: *const c_char, _: *mut c_void) {
    if name.is_null() {
        return;
    }
    // SAFETY: the C ABI contract guarantees NUL-terminated strings that live
    // for the duration of the call.
    let name = unsafe { CStr::from_ptr(name) }
        .to_string_lossy()
        .into_owned();
    let attrs = if attrs_json.is_null() {
        serde_json::Map::new()
    } else {
        // SAFETY: as above.
        let json = unsafe { CStr::from_ptr(attrs_json) }.to_string_lossy();
        match serde_json::from_str(&json) {
            Ok(serde_json::Value::Object(map)) => map,
            _ => {
                bevy::log::warn!("kag: tag `{name}` has invalid attrs JSON: {json}");
                serde_json::Map::new()
            }
        }
    };
    // `try_with` so a tag reported during thread teardown is dropped, not a panic.
    let _ = PENDING_TAGS.try_with(|q| q.borrow_mut().push(RawTag { name, attrs }));
}

/// Owning handle to a KAG parser. Non-send resource.
///
/// Must be dropped before the [`Tjs2Engine`] it was created from.
pub struct KagParser {
    ptr: *mut sys::KAGParserOpaque,
}

impl KagParser {
    pub fn new(engine: &Tjs2Engine) -> Result<Self, KagError> {
        // SAFETY: engine.as_ptr() is a live interpreter on this thread.
        let ptr = unsafe { sys::krkr_kag_create(engine.as_ptr()) };
        if ptr.is_null() {
            return Err(KagError::CreateFailed(krkr_script::last_error()));
        }
        // SAFETY: ptr is live; the callback is a 'static fn and needs no userdata.
        unsafe { sys::krkr_kag_set_tag_callback(ptr, Some(tag_callback), std::ptr::null_mut()) };
        Ok(Self { ptr })
    }

    /// Load a scenario such as `"first.ks"` from the mounted storage.
    pub fn load(&mut self, storage: &str) -> Result<(), KagError> {
        let storage = CString::new(storage)?;
        // SAFETY: ptr is live; storage is NUL-terminated.
        let code = unsafe { sys::krkr_kag_load_script(self.ptr, storage.as_ptr()) };
        check(code, "load_script")
    }

    /// Jump to `label` in `storage`; `None` keeps the current scenario / start.
    pub fn jump(&mut self, storage: Option<&str>, label: Option<&str>) -> Result<(), KagError> {
        let storage = storage.map(CString::new).transpose()?;
        let label = label.map(CString::new).transpose()?;
        let as_ptr = |s: &Option<CString>| s.as_ref().map_or(std::ptr::null(), |s| s.as_ptr());
        // SAFETY: ptr is live; strings are NUL-terminated or NULL as allowed.
        let code = unsafe { sys::krkr_kag_jump(self.ptr, as_ptr(&storage), as_ptr(&label)) };
        check(code, "jump")
    }

    /// Parse the next tag in C++ and collect what the callback reported.
    pub fn step(&mut self) -> Result<Step, KagError> {
        PENDING_TAGS.with(|q| q.borrow_mut().clear());
        // SAFETY: ptr is live; the callback only touches PENDING_TAGS.
        let code = unsafe { sys::krkr_kag_process_next(self.ptr) };
        let tags = PENDING_TAGS.with(|q| std::mem::take(&mut *q.borrow_mut()));
        match code {
            c if c == sys::KRKR_OK => Ok(Step::Tags(tags)),
            c if c == sys::KRKR_END => Ok(Step::End),
            _ => Err(KagError::Ffi {
                what: "process_next",
                message: krkr_script::last_error(),
            }),
        }
    }
}

impl Drop for KagParser {
    fn drop(&mut self) {
        // SAFETY: ptr came from krkr_kag_create and is destroyed once.
        unsafe { sys::krkr_kag_destroy(self.ptr) };
    }
}

fn check(code: sys::krkr_result_t, what: &'static str) -> Result<(), KagError> {
    if code == sys::KRKR_OK {
        Ok(())
    } else {
        Err(KagError::Ffi {
            what,
            message: krkr_script::last_error(),
        })
    }
}
