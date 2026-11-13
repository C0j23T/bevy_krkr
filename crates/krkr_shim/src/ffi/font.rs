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
