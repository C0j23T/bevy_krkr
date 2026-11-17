mod common;

use bevy_krkr::{
    script::{Tjs2Engine, last_error},
    sys,
};
use std::{ffi::CString, path::PathBuf, time::Duration};

struct Session;
impl Drop for Session {
    fn drop(&mut self) {
        unsafe { sys::krkr_engine_shutdown() };
    }
}

#[test]
#[ignore = "requires local snbk archives and native plugins in the game's plugin directory"]
fn startup_and_scene() {
    common::initialize_renderer();
    let data = std::env::var_os("SNBK_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test_games/snbk/data.xp3")
        });
    let path = CString::new(data.to_string_lossy().as_bytes()).unwrap();
    assert_eq!(
        unsafe { sys::krkr_engine_init(path.as_ptr()) },
        sys::KRKR_OK,
        "{}",
        last_error()
    );
    let _session = Session;
    let engine = Tjs2Engine::new().unwrap();
    let mut archives: Vec<_> = std::fs::read_dir(data.parent().unwrap())
        .unwrap()
        .map(|p| p.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "xp3"))
        .collect();
    archives.sort();
    for archive in archives {
        let storage = format!(
            "file://{}>",
            std::fs::canonicalize(archive).unwrap().display()
        );
        engine
            .exec_script(&format!("Storages.addAutoPath({storage:?});"), "snbk mount")
            .unwrap();
    }
    if let Some(probe) = std::env::var_os("SNBK_BEFORE_STARTUP") {
        engine
            .exec_script(
                &std::fs::read_to_string(probe).unwrap(),
                "snbk before startup",
            )
            .unwrap();
    }
    let result = engine.exec_startup();
    if std::env::var_os("SNBK_DUMP_SCRIPTS").is_some() {
        engine
            .exec_script("Scripts.dump();", "snbk bytecode diagnostics")
            .unwrap();
    }
    if let Some(probe) = std::env::var_os("SNBK_PROBE") {
        engine
            .exec_script(&std::fs::read_to_string(probe).unwrap(), "snbk probe")
            .unwrap();
    }
    let log = engine.eval("Debug.getLastLog(100000)").unwrap();
    let output = std::env::var_os("SNBK_CAPTURE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("snbk-acceptance"));
    std::fs::create_dir_all(&output).unwrap();
    std::fs::write(output.join("startup.log"), log.to_string()).unwrap();
    let (mut width, mut height) = (0, 0);
    let texture = unsafe { sys::krkr_scene_build(std::ptr::null_mut(), &mut width, &mut height) };
    if texture != 0 {
        let pixels = bevy_krkr::plugin::render::read_pixels(texture).unwrap();
        let mut ppm = format!("P6\n{width} {height}\n255\n").into_bytes();
        for pixel in pixels.chunks_exact(4) {
            ppm.extend_from_slice(&pixel[..3]);
        }
        std::fs::write(output.join("startup.ppm"), ppm).unwrap();
    }
    result.unwrap();
    for _ in 0..120 {
        assert_eq!(
            unsafe { sys::krkr_engine_tick() },
            sys::KRKR_OK,
            "{}",
            last_error()
        );
        let (mut width, mut height) = (0, 0);
        let texture =
            unsafe { sys::krkr_scene_build(std::ptr::null_mut(), &mut width, &mut height) };
        assert_ne!(texture, 0, "{}", last_error());
        std::thread::sleep(Duration::from_millis(10));
    }
}
