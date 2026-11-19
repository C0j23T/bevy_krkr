/// Borrow the host fallback font for the process lifetime; null if unconfigured.
///
/// # Safety
/// `len` must point to a writable usize.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_host_fallback_font(len: *mut usize) -> *const u8 {
    if len.is_null() {
        return std::ptr::null();
    }
    let bytes = crate::font::fallback_font();
    *len = bytes.len();
    if bytes.is_empty() {
        std::ptr::null()
    } else {
        bytes.as_ptr()
    }
}

use super::guard;
use crate::font::{self, PositionedGlyph};
use std::ffi::c_void;

type FontVisitor = unsafe extern "C" fn(*mut c_void, *const u8, usize, u32) -> bool;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_host_font_list(context: *mut c_void, visit: FontVisitor) -> bool {
    guard(false, || {
        font::families()
            .into_iter()
            .all(|name| unsafe { visit(context, name.as_ptr(), name.len(), 0) })
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_host_font_face(
    name: *const u16,
    count: usize,
    style: u32,
    context: *mut c_void,
    visit: FontVisitor,
) -> bool {
    guard(false, || {
        if name.is_null() {
            return false;
        }
        let family = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(name, count) });
        font::with_system_font(&family, style, |bytes, index| unsafe {
            visit(context, bytes.as_ptr(), bytes.len(), index)
        })
        .unwrap_or(false)
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_host_shape_line(
    bytes: *const u8,
    len: usize,
    index: u32,
    text: *const u16,
    count: usize,
    output: *mut *mut PositionedGlyph,
    glyph_count: *mut usize,
) -> bool {
    guard(false, || {
        if bytes.is_null() || text.is_null() || output.is_null() || glyph_count.is_null() {
            return false;
        }
        let text = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(text, count) });
        let Some(glyphs) = font::shape_line(
            unsafe { std::slice::from_raw_parts(bytes, len) },
            index,
            &text,
        ) else {
            return false;
        };
        let slice = glyphs.into_boxed_slice();
        unsafe {
            *glyph_count = slice.len();
            *output = Box::into_raw(slice).cast();
        }
        true
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_host_release_glyphs(output: *mut PositionedGlyph, count: usize) {
    if !output.is_null() {
        drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(output, count)) });
    }
}
