//! Every plugin in this build attaches when a script links it.
//!
//! The plugins are compiled into the engine, so `Plugins.link` only attaches
//! what the build kept: a plugin whose translation unit the linker dropped is
//! indistinguishable from one that was never built. This checks the ones with
//! a TJS surface, and that linking an unknown module is harmless.

use std::ffi::CString;
use std::path::PathBuf;

use bevy_krkr::script::{last_error, Tjs2Engine};
use bevy_krkr::sys;

fn data_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/game")
}

/// One engine per process, so everything runs in a single test.
#[test]
fn every_plugin_attaches_on_link() {
    let path = data_path();
    let c_path = CString::new(path.to_string_lossy().as_bytes()).expect("no NUL in the path");
    // SAFETY: called on the test thread, once for this process.
    let code = unsafe {
        sys::krkr_engine_set_screen_size(640, 480);
        sys::krkr_engine_init(c_path.as_ptr())
    };
    assert_eq!(code, sys::KRKR_OK, "krkr_engine_init: {}", last_error());
    let engine = Tjs2Engine::new().expect("Tjs2Engine::new");

    // `(module, a member the module adds, where)`.
    let expected: &[(&str, &str)] = &[
        ("layerExImage.dll", "Layer.light"),
        ("perspective.dll", "Layer.perspectiveCopy"),
        ("layerExRaster.dll", "Layer.copyRaster"),
        ("layerExSave.dll", "Layer.saveLayerImagePng"),
        ("layerExMovie.dll", "Layer.openMovie"),
        ("addFont.dll", "System.addFont"),
        ("csvParser.dll", "global.CSVParser"),
        ("fftgraph.dll", "global.drawFFTGraph"),
        ("getabout.dll", "System.getAboutString"),
        ("saveStruct.dll", "(new Array()).toStructString"),
        ("win32dialog.dll", "global.WIN32Dialog"),
    ];
    for (module, member) in expected {
        let before = engine.eval(&format!("typeof {member}")).unwrap();
        engine
            .exec_script(&format!("Plugins.link(\"{module}\");"), "link")
            .unwrap_or_else(|e| panic!("Plugins.link({module}): {e}"));
        let after = engine.eval(&format!("typeof {member}")).unwrap();
        assert_ne!(before, "Object", "{module}: {member} existed before linking");
        assert_eq!(after, "Object", "{module}: {member} is missing after linking");
    }

    // `Scripts.foreach` / `Scripts.getObjectKeys` come from the module the
    // engine attaches itself.
    assert_eq!(engine.eval("typeof Scripts.foreach").unwrap(), "Object");
    assert_eq!(engine.eval("typeof Scripts.getObjectKeys").unwrap(), "Object");

    // xp3filter is attached at start-up because it has to be up before the
    // first archive is opened; nothing observable in TJS, so linking it again
    // must at least not throw.
    engine
        .exec_script(r#"Plugins.link("xp3filter.dll");"#, "link")
        .expect("Plugins.link(xp3filter.dll)");

    // KAGParserEx semantics are built into the single core KAGParser class.
    engine
        .exec_script(r#"Plugins.link("KAGParserEx.dll");"#, "link")
        .expect("Plugins.link(KAGParserEx.dll)");
    assert_eq!(engine.eval("typeof (new KAGParser()).paramMacros").unwrap(), "Object");
}
