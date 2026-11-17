mod common;

use bevy_krkr::{script::Tjs2Engine, sys};
use std::ffi::CString;

static ENGINE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn motionplayer_alias_registers_motion_api() {
    let _guard = ENGINE_LOCK.lock().unwrap();
    common::initialize_renderer();
    let path = CString::new(format!("{}/assets/game", env!("CARGO_MANIFEST_DIR"))).unwrap();
    assert_eq!(
        unsafe { sys::krkr_engine_init(path.as_ptr()) },
        sys::KRKR_OK
    );
    let engine = Tjs2Engine::new().unwrap();
    assert_eq!(engine.eval("typeof global.Motion").unwrap(), "undefined");
    engine
        .exec_script(
            "Plugins.link('motionplayer.dll'); Plugins.link('emoteplayer.dll');",
            "Emote alias",
        )
        .unwrap();
    for member in ["ResourceManager", "EmotePlayer", "Player", "D3DAdaptor"] {
        assert_eq!(
            engine.eval(&format!("typeof Motion.{member}")).unwrap(),
            "Object"
        );
    }
    drop(engine);
    unsafe { sys::krkr_engine_shutdown() };
}

#[test]
#[ignore = "requires local test_games/snbk/data720.xp3"]
fn snbk_motion_resource_renders() {
    let _guard = ENGINE_LOCK.lock().unwrap();
    common::initialize_renderer();
    let path = CString::new(format!("{}/assets/game", env!("CARGO_MANIFEST_DIR"))).unwrap();
    assert_eq!(
        unsafe { sys::krkr_engine_init(path.as_ptr()) },
        sys::KRKR_OK
    );
    let engine = Tjs2Engine::new().unwrap();
    let run = |s: &str| engine.exec_script(s, "snbk motion").unwrap();
    run(
        "Plugins.link('xp3filter.dll'); Plugins.link('motionplayer.dll'); global.manager = new Motion.ResourceManager(null, 0);",
    );
    let sample = format!(
        "{}/test_games/snbk/data720.xp3>sd001.mtn",
        env!("CARGO_MANIFEST_DIR")
    );
    run(&format!("global.root = manager.load({sample:?});"));
    run(
        "global.w = new Window(); w.setInnerSize(1280,720); w.visible=true; global.layer = new Layer(w,null); layer.setImageSize(1280,720); layer.setSizeToImageSize(); layer.type=ltAlpha; layer.visible=true; global.player = new Motion.Player(manager); player.chara='SD001'; player.play('SD001AA',0); player.progress(0); player.draw(layer);",
    );
    assert_eq!(engine.eval("player.playing").unwrap(), "1");
    let frame = || {
        let (mut width, mut height) = (0, 0);
        let id = unsafe { sys::krkr_scene_build(std::ptr::null_mut(), &mut width, &mut height) };
        assert_ne!(id, 0);
        bevy_krkr::plugin::render::read_pixels(id).unwrap()
    };
    let pixels = frame();
    assert!(
        pixels
            .chunks_exact(4)
            .any(|p| p[0] != 0 || p[1] != 0 || p[2] != 0)
    );
    run("player.progress(100); player.setCoord(100,0); player.draw(layer);");
    assert!(
        frame() != pixels,
        "moving the player must update rendered pixels"
    );
    drop(engine);
    unsafe { sys::krkr_engine_shutdown() };
}
