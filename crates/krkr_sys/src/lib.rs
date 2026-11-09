//! Raw FFI bindings for Kirikiroid2.
//!
//! # Safety
//! All functions in this crate are `unsafe`. Safe wrappers are provided by
//! the higher-level crates: `krkr_script`, `krkr_kag`, `krkr_plugin`.
//!
//! # Codegen
//! This crate's content is **entirely generated** by `build.rs`.
//! - Without `KIRIKIROID2_SRC` / `real_ffi` feature: stub implementations.
//! - With both set: real `bindgen`-generated bindings to Kirikiroid2.
//!
//! Do NOT hand-write Kirikiroid2 functionality here.

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(dead_code)]
#![allow(clippy::all)]

#[used]
static KRKR_SHIM_LINKED: extern "C" fn() = krkr_shim::krkr_shim_present;

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
