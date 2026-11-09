//! Rust implementations behind the Kirikiroid2 core's C ABI.

pub mod archives;
pub mod ffi;
pub mod render;

pub extern "C" fn krkr_shim_present() {}
