//! Host-supplied fallback font. Static bytes remain valid for every engine session.
use std::sync::OnceLock;

static FALLBACK: OnceLock<&'static [u8]> = OnceLock::new();

pub fn set_fallback_font(bytes: &'static [u8]) {
    let _ = FALLBACK.set(bytes);
}

pub(crate) fn fallback_font() -> &'static [u8] {
    FALLBACK.get().copied().unwrap_or_default()
}
