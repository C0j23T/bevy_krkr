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
        ("windowEx.dll", "Window.getWindowRect"),
        ("gfxFire.dll", "global.gfxFire"),
        ("layerExAreaAverage.dll", "Layer.stretchCopyAA"),
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

    // windowEx reports the host's screen texture, not the Win32 window frame:
    // x/y are 0 and the size is the front window's paint box.
    // SAFETY: this test process is single-threaded while the variable is set.
    unsafe { std::env::set_var("BEVY_KRKR_WINDOW_EX", "42") };
    engine
        .exec_script(
            "global.wexWindow = new Window(); wexWindow.setInnerSize(320, 200); wexWindow.visible = true;",
            "windowEx window",
        )
        .unwrap();
    for (expression, expected) in [
        ("wexWindow.getWindowRect().x", "0"),
        ("wexWindow.getWindowRect().y", "0"),
        ("wexWindow.getWindowRect().w", "320"),
        ("wexWindow.getWindowRect().h", "200"),
        ("wexWindow.getClientRect().w", "320"),
        ("wexWindow.getClientRect().h", "200"),
        ("System.getDisplayMonitors().count", "1"),
        ("System.getDisplayMonitors()[0].x", "0"),
        ("System.getDisplayMonitors()[0].y", "0"),
        ("System.getDisplayMonitors()[0].w", "320"),
        ("System.getDisplayMonitors()[0].h", "200"),
        ("System.getDisplayMonitors()[0].primary", "1"),
        ("System.getDisplayMonitors()[0].monitor.w", "320"),
        ("System.getDisplayMonitors()[0].work.h", "200"),
        ("System.getDisplayMonitors(10, 10, 100, 100).count", "1"),
        ("System.getDisplayMonitors(400, 400, 100, 100).count", "0"),
        ("System.getMonitorInfo().primary", "1"),
        ("System.getMonitorInfo().monitor.w", "320"),
        ("System.getMonitorInfo().work.h", "200"),
        ("System.getSystemMetrics('CXSCREEN')", "320"),
        ("System.getSystemMetrics('CYSCREEN')", "200"),
        ("System.getSystemMetrics('CMONITORS')", "1"),
        ("System.getDoubleClickTime()", "500"),
        ("System.readEnvValue('BEVY_KRKR_WINDOW_EX')", "42"),
        ("System.expandEnvString('%BEVY_KRKR_WINDOW_EX%')", "42"),
        ("Window.getNotificationNum('PAINT')", "15"),
        ("Window.getNotificationName(15)", "PAINT"),
        ("Scripts.eval('1 + 2')", "3"),
    ] {
        assert_eq!(engine.eval(expression).unwrap(), expected, "{expression}");
    }
    engine
        .exec_script("System.setCursorPos(7, 9);", "windowEx cursor")
        .unwrap();
    assert_eq!(engine.eval("System.getCursorPos().x").unwrap(), "7");
    assert_eq!(engine.eval("System.getCursorPos().y").unwrap(), "9");
    engine
        .exec_script("wexWindow.visible = false; invalidate wexWindow;", "windowEx cleanup")
        .unwrap();

    assert_ne!(engine.eval("typeof Storages.fstat").unwrap(), "Object");
    engine
        .exec_script(r#"Plugins.link("fstat.dll");"#, "link fstat")
        .unwrap();
    assert_eq!(engine.eval("typeof Storages.fstat").unwrap(), "Object");
    assert_eq!(
        engine.eval("Storages.fstat('bgm.wav').size").unwrap(),
        "88244"
    );
    for member in [
        "moveFile",
        "deleteFile",
        "copyFile",
        "createDirectory",
        "removeDirectory",
        "isExistentDirectory",
    ] {
        assert_eq!(
            engine.eval(&format!("typeof Storages.{member}")).unwrap(),
            "Object"
        );
    }

    let test_dir = std::env::temp_dir().join(format!("bevy_krkr_fstat_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&test_dir);
    std::fs::create_dir(&test_dir).expect("create fstat test directory");
    let storage_name =
        |path: &std::path::Path| format!("file://./{}", path.strip_prefix("/").unwrap().display());
    let source = test_dir.join("source.txt");
    let moved = test_dir.join("moved.txt");
    let copied = test_dir.join("copied.txt");
    std::fs::write(&source, "fstat").expect("write fstat test file");
    engine
        .exec_script(
            &format!(
                "Storages.moveFile({:?}, {:?});",
                storage_name(&source),
                storage_name(&moved)
            ),
            "fstat move",
        )
        .unwrap();
    assert!(!source.exists());
    assert_eq!(std::fs::read_to_string(&moved).unwrap(), "fstat");
    engine
        .exec_script(
            &format!(
                "Storages.copyFile({:?}, {:?}, false);",
                storage_name(&moved),
                storage_name(&copied)
            ),
            "fstat copy",
        )
        .unwrap();
    assert_eq!(std::fs::read_to_string(&copied).unwrap(), "fstat");
    engine
        .exec_script(
            &format!("Storages.deleteFile({:?});", storage_name(&copied)),
            "fstat delete",
        )
        .unwrap();
    assert!(!copied.exists());

    let created_dir = test_dir.join("created");
    let created_dir_name = format!("{}/", storage_name(&created_dir));
    engine
        .exec_script(
            &format!("Storages.createDirectory({created_dir_name:?});"),
            "fstat mkdir",
        )
        .unwrap();
    assert!(created_dir.is_dir());
    assert_eq!(
        engine
            .eval(&format!(
                "Storages.isExistentDirectory({created_dir_name:?})"
            ))
            .unwrap(),
        "1"
    );
    engine
        .exec_script(
            &format!("Storages.removeDirectory({created_dir_name:?});"),
            "fstat rmdir",
        )
        .unwrap();
    assert!(!created_dir.exists());
    std::fs::remove_dir_all(test_dir).expect("clean up fstat test directory");

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
    for name in ["KAGParserEx.dll", "kagparserex.dll", "plugin/KAGPARSEREX.DLL"] {
        engine
            .exec_script(&format!("Plugins.link({name:?});"), "link")
            .expect("link built-in KAGParserEx");
    }
    assert_eq!(engine.eval("typeof (new KAGParser()).paramMacros").unwrap(), "Object");

    engine
        .exec_script(r#"Plugins.link("PackinOne.dll");"#, "link PackinOne")
        .expect("Plugins.link(PackinOne.dll)");
    for member in [
        "fillAlpha",
        "copyRightBlueToLeftAlpha",
        "copyBottomBlueToTopAlpha",
        "copyAlphaToProvince",
        "clipAlphaRect",
        "fillByProvince",
        "shrinkCopy",
        "shrinkCopyFast",
    ] {
        assert_eq!(
            engine.eval(&format!("typeof Layer.{member}")).unwrap(),
            "Object",
            "PackinOne: Layer.{member}"
        );
    }
    assert_eq!(engine.eval("typeof System.addFont").unwrap(), "Object");
    assert_eq!(engine.eval("typeof Layer.light").unwrap(), "Object");
    assert_eq!(engine.eval("typeof Layer.copyRaster").unwrap(), "Object");
}
