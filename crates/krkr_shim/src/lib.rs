//! Rust implementations behind the Kirikiroid2 core's C ABI.

pub mod archives;
pub mod ffi;
pub mod jpeg;
pub mod png;
mod save_image;
pub mod render;
pub mod video;
pub mod xml;

pub extern "C" fn krkr_shim_present() {}
