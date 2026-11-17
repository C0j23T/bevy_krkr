use std::ffi::CString;

use bevy_krkr::{script::Tjs2Engine, sys};

#[test]
fn ignored_plugin_link_and_unlink_are_noops() {
    let path = CString::new(format!("{}/assets/game", env!("CARGO_MANIFEST_DIR"))).unwrap();
    assert_eq!(unsafe { sys::krkr_engine_init(path.as_ptr()) }, sys::KRKR_OK);
    let engine = Tjs2Engine::new().unwrap();

    engine
        .exec_script(
            r#"Plugins.link("plugin/UTIL.DLL");"#,
            "link ignored util plugin",
        )
        .unwrap();
    assert_eq!(engine.eval(r#"Plugins.unlink("util.dll")"#).unwrap(), "1");

    drop(engine);
    unsafe { sys::krkr_engine_shutdown() };
}
