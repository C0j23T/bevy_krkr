//! `krkr_script` – TJS2 runtime adapter.
//!
//! [`Tjs2Engine`] is a handle to the Kirikiroid2 engine's TJS2 interpreter
//! (the one with every native class registered: Layer, KAGParser, Storages…).
//! It is a Bevy **non-send** resource: the interpreter is single-threaded, so
//! it must only be touched from the main thread. The raw pointer field makes
//! the type `!Send + !Sync`, which Bevy enforces at compile time.
//!
//! The engine is not created by [`Tjs2Plugin`] itself. `krkr_plugin` creates
//! it when a `KirikiriEnterEvent` arrives and removes it on
//! `KirikiriExitEvent`. While it exists, [`Tjs2Plugin`] drives
//! `krkr_engine_tick` once per frame.

use std::ffi::{c_char, CStr, CString, NulError};

use bevy::prelude::*;
use krkr_sys as sys;

/// Errors returned by [`Tjs2Engine`].
#[derive(Debug, thiserror::Error)]
pub enum Tjs2Error {
    #[error("krkr_tjs_create failed: {0}")]
    InitFailed(String),
    #[error("TJS2 {what} failed: {message}")]
    Script { what: &'static str, message: String },
    #[error("string passed to TJS2 contains a NUL byte: {0}")]
    InvalidString(#[from] NulError),
}

/// Message of the last failed `krkr_sys` call on this thread.
pub fn last_error() -> String {
    // SAFETY: krkr_last_error returns NULL or a NUL-terminated string that
    // stays valid until the next krkr_sys call on this thread; it is copied.
    unsafe {
        let p = sys::krkr_last_error();
        if p.is_null() {
            "unknown error".to_owned()
        } else {
            CStr::from_ptr(p).to_string_lossy().into_owned()
        }
    }
}

/// Take ownership of a string returned by `krkr_tjs_eval` / `krkr_tjs_call`.
///
/// # Safety
/// `p` must be NULL or a string allocated by the C ABI and not yet freed.
unsafe fn take_string(p: *mut c_char) -> String {
    if p.is_null() {
        return String::new();
    }
    // SAFETY: per the contract above; freed exactly once with krkr_free.
    unsafe {
        let s = CStr::from_ptr(p).to_string_lossy().into_owned();
        sys::krkr_free(p.cast());
        s
    }
}

/// Handle to the engine's TJS2 interpreter. Non-send resource.
pub struct Tjs2Engine {
    ptr: *mut sys::tTJSOpaque,
}

impl Tjs2Engine {
    /// Get the interpreter. Call after `krkr_engine_init`, on the main thread.
    pub fn new() -> Result<Self, Tjs2Error> {
        // SAFETY: no preconditions beyond engine init; NULL is handled.
        let ptr = unsafe { sys::krkr_tjs_create() };
        if ptr.is_null() {
            Err(Tjs2Error::InitFailed(last_error()))
        } else {
            Ok(Self { ptr })
        }
    }

    /// Raw pointer for passing to other `krkr_sys` calls. Borrowed, not owned.
    pub fn as_ptr(&self) -> *mut sys::tTJSOpaque {
        self.ptr
    }

    /// Execute TJS2 source. `context` names the source in error messages.
    pub fn exec_script(&self, script: &str, context: &str) -> Result<(), Tjs2Error> {
        let script = CString::new(script)?;
        let context = CString::new(context)?;
        // SAFETY: self.ptr is a live interpreter; both strings are NUL-terminated.
        let code =
            unsafe { sys::krkr_tjs_exec_script(self.ptr, script.as_ptr(), context.as_ptr()) };
        check(code, "exec_script")
    }

    /// Evaluate a TJS2 expression; returns its value converted to a string.
    pub fn eval(&self, expr: &str) -> Result<String, Tjs2Error> {
        let expr = CString::new(expr)?;
        let mut out: *mut c_char = std::ptr::null_mut();
        // SAFETY: self.ptr is a live interpreter; `out` is a valid out-pointer.
        let code = unsafe { sys::krkr_tjs_eval(self.ptr, expr.as_ptr(), &mut out) };
        // SAFETY: on success `out` is NULL or an owned string from the C ABI.
        let value = unsafe { take_string(out) };
        check(code, "eval").map(|()| value)
    }

    /// Call the function `func_expr` evaluates to (e.g. `"obj.method"`) with
    /// string arguments; returns the result converted to a string. Arguments
    /// are passed as values, never spliced into source.
    pub fn call(&self, func_expr: &str, args: &[&str]) -> Result<String, Tjs2Error> {
        let func = CString::new(func_expr)?;
        let args: Vec<CString> = args.iter().map(|a| CString::new(*a)).collect::<Result<_, _>>()?;
        let ptrs: Vec<*const c_char> = args.iter().map(|a| a.as_ptr()).collect();
        let mut out: *mut c_char = std::ptr::null_mut();
        // SAFETY: self.ptr is live; `ptrs` holds `ptrs.len()` NUL-terminated
        // strings that outlive the call; `out` is a valid out-pointer.
        let code = unsafe {
            sys::krkr_tjs_call(self.ptr, func.as_ptr(), ptrs.as_ptr(), ptrs.len(), &mut out)
        };
        // SAFETY: as in `eval`.
        let value = unsafe { take_string(out) };
        check(code, "call").map(|()| value)
    }
}

impl Drop for Tjs2Engine {
    fn drop(&mut self) {
        // SAFETY: self.ptr came from krkr_tjs_create and is released once.
        unsafe { sys::krkr_tjs_destroy(self.ptr) };
    }
}

fn check(code: sys::krkr_result_t, what: &'static str) -> Result<(), Tjs2Error> {
    if code == sys::KRKR_OK {
        Ok(())
    } else {
        Err(Tjs2Error::Script {
            what,
            message: last_error(),
        })
    }
}

/// System set for TJS2 work inside `Update`. KAG systems run after it.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct Tjs2Set;

/// Drives the TJS2 runtime every frame while a [`Tjs2Engine`] exists.
#[derive(Default)]
pub struct Tjs2Plugin;

impl Plugin for Tjs2Plugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, tjs2_tick.in_set(Tjs2Set));
    }
}

fn tjs2_tick(engine: Option<NonSend<Tjs2Engine>>) {
    if engine.is_none() {
        return;
    }
    // SAFETY: an engine exists, so krkr_engine_init has succeeded; main thread
    // is guaranteed by the NonSend parameter.
    let code = unsafe { sys::krkr_engine_tick() };
    if code != sys::KRKR_OK {
        warn!("bevy_krkr: krkr_engine_tick: {}", last_error());
    }
}
