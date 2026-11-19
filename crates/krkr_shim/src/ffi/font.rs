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

#[repr(C)]
pub struct DrawingFace {
    bytes: *const u8,
    length: usize,
    index: u32,
}
type GlyphVisitor =
    unsafe extern "C" fn(*mut c_void, *const u8, usize, u32, *const PositionedGlyph, usize) -> bool;

/// # Safety
/// Font bytes and UTF-16 text must remain readable until all synchronous callbacks return.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_host_shape_line(
    faces: *const DrawingFace,
    face_count: usize,
    text: *const u16,
    count: usize,
    style: u32,
    context: *mut c_void,
    visit: GlyphVisitor,
) -> bool {
    guard(false, || {
        if faces.is_null() || text.is_null() || !(1..=513).contains(&face_count) {
            return false;
        }
        let faces: Option<Vec<_>> = unsafe { std::slice::from_raw_parts(faces, face_count) }
            .iter()
            .map(|f| {
                (!f.bytes.is_null()).then(|| {
                    (
                        unsafe { std::slice::from_raw_parts(f.bytes, f.length) },
                        f.index,
                    )
                })
            })
            .collect();
        let Some(faces) = faces else {
            return false;
        };
        let text = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(text, count) });
        font::shape_runs(&faces, &text, style, |bytes, index, glyphs| {
            if unsafe {
                visit(
                    context,
                    bytes.as_ptr(),
                    bytes.len(),
                    index,
                    glyphs.as_ptr(),
                    glyphs.len(),
                )
            } {
                Ok(())
            } else {
                Err("glyph callback failed".into())
            }
        })
        .is_ok()
    })
}
