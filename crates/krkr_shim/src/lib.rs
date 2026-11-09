//! Rust implementations behind the Kirikiroid2 core's C ABI.

pub mod archives;
pub mod ffi;
pub mod jpeg;
pub mod png;
pub mod render;
pub mod xml;

pub extern "C" fn krkr_shim_present() {}
