#![allow(unsafe_op_in_unsafe_fn)]

//! The C ABI of this crate, one file per area under `ffi/`.
//!
//! # Rules for this layer
//!
//! - Validate everything that crosses the boundary; C++ passes null and zero
//!   lengths for empty inputs.
//! - Never unwind into C++: the engine is compiled without the tables a Rust
//!   panic needs, so a panic crossing back would abort the process. Entry
//!   points that can panic catch it and report failure.
//! - Report errors as a sentinel (`0` for the codecs, `SIZE_MAX` for the
//!   `usize` returns), because C has no way to receive a `Result`.

mod archives;
mod config;
mod jpeg;
mod lz4;
mod png;
mod save_image;
mod image_ops;
mod webp;
mod xxhash;
mod zlib;

pub use archives::*;
pub use config::*;
pub use jpeg::*;
pub use lz4::*;
pub use png::*;
pub use save_image::*;
pub use image_ops::*;
pub use webp::*;
pub use xxhash::*;
pub use zlib::*;

use std::ffi::c_void;
use std::panic::{AssertUnwindSafe, catch_unwind};

/// Borrows a C buffer as a slice. A null pointer or a zero length is the empty
/// slice: the engine passes `(nullptr, 0)` for empty inputs.
///
/// # Safety
/// `ptr` must point to `len` readable bytes, or be null when `len == 0`.
unsafe fn input<'a>(ptr: *const c_void, len: usize) -> &'a [u8] {
    if ptr.is_null() || len == 0 {
        &[]
    } else {
        std::slice::from_raw_parts(ptr as *const u8, len)
    }
}

/// Borrows a C buffer as a mutable slice, with the same empty-input rule as
/// [`input`].
///
/// # Safety
/// `ptr` must point to `len` writable bytes, or be null when `len == 0`.
unsafe fn output<'a>(ptr: *mut c_void, len: usize) -> &'a mut [u8] {
    if ptr.is_null() || len == 0 {
        // An empty slice still needs a non-null, aligned pointer.
        std::slice::from_raw_parts_mut(std::ptr::NonNull::dangling().as_ptr(), 0)
    } else {
        std::slice::from_raw_parts_mut(ptr as *mut u8, len)
    }
}

/// Runs `body`, turning a panic into `fallback`.
fn guard<T>(fallback: T, body: impl FnOnce() -> T) -> T {
    catch_unwind(AssertUnwindSafe(body)).unwrap_or(fallback)
}

/// Error value of the `usize`-returning entry points.
const FAILED: usize = usize::MAX;

/// A NUL-terminated copy to hand to a visitor. A string that is not UTF-8
/// becomes empty rather than failing the whole file.
fn c_string(text: &str) -> std::ffi::CString {
    std::ffi::CString::new(text).unwrap_or_default()
}
