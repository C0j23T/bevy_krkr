use bevy_krkr::{script::Tjs2Engine, sys};
use std::ffi::CString;

#[test]
fn utf8_utf16_roundtrip_and_invalid_sequences() {
    let project = std::env::temp_dir().join(format!("krkr-unicode-{}", std::process::id()));
    std::fs::create_dir_all(&project).unwrap();
    let path = CString::new(project.to_str().unwrap()).unwrap();
    assert_eq!(
        unsafe { sys::krkr_engine_init(path.as_ptr()) },
        sys::KRKR_OK
    );
    let engine = Tjs2Engine::new().unwrap();
    engine
        .exec_script(
            "global.unicode='ASCII 日本 🙂 𐀀 􏿿'; global.afterUnicode=42;",
            "unicode",
        )
        .unwrap();
    assert_eq!(engine.eval("unicode").unwrap(), "ASCII 日本 🙂 𐀀 􏿿");
    assert_eq!(engine.eval("afterUnicode").unwrap(), "42");
    assert_eq!(engine.eval("'🙂'.length").unwrap(), "2");
    assert_eq!(engine.eval(r"'\xd83d\xde42'").unwrap(), "🙂");
    for invalid in [
        &[0xc0, 0x80][..],
        &[0xe0, 0x80, 0x80][..],
        &[0xed, 0xa0, 0x80][..],
        &[0xf0, 0x80, 0x80, 0x80][..],
        &[0xf4, 0x90, 0x80, 0x80][..],
        &[0xf0, 0x9f, 0x99][..],
    ] {
        let source = [b"global.afterUnicode=0; '", invalid, b"';"].concat();
        let source = CString::new(source).unwrap();
        let context = c"invalid UTF-8";
        assert_ne!(
            unsafe {
                sys::krkr_tjs_exec_script(engine.as_ptr(), source.as_ptr(), context.as_ptr())
            },
            sys::KRKR_OK
        );
        assert_eq!(engine.eval("afterUnicode").unwrap(), "42");
    }
    for surrogate in [r"'\xd800'", r"'\xdc00'", r"'\xd800x'"] {
        assert_eq!(
            engine.eval(surrogate).unwrap(),
            "",
            "encoded isolated surrogate: {surrogate}"
        );
    }
    drop(engine);
    unsafe { sys::krkr_engine_shutdown() };
    std::fs::remove_dir_all(project).unwrap();
}
