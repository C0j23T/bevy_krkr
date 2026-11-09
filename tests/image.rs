//! Images reach the engine as PNG and leave it the same way: `LoadPNG.cpp`
//! hands both directions to `krkr_shim`. The engine is initialised once per
//! process, so this is one test.

mod common;

use std::ffi::CString;
use std::path::PathBuf;

use bevy_krkr::script::{Tjs2Engine, last_error};
use bevy_krkr::sys;

#[test]
fn a_layer_survives_a_png_round_trip() {
    common::initialize_renderer();

    let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("image_game");
    std::fs::create_dir_all(path.join("savedata")).expect("create the game directory");
    let c_path = CString::new(path.to_string_lossy().as_bytes()).expect("no NUL in the path");
    // SAFETY: called on the test thread, once for this process.
    let code = unsafe {
        sys::krkr_engine_set_screen_size(64, 48);
        sys::krkr_engine_init(c_path.as_ptr())
    };
    assert_eq!(code, sys::KRKR_OK, "krkr_engine_init: {}", last_error());
    let engine = Tjs2Engine::new().expect("Tjs2Engine::new");

    engine
        .exec_script(
            r#"
            global.w = new Window(); w.setInnerSize(64,48); w.visible = true;
            global.l = new Layer(w,null); l.setImageSize(3,1); l.setSizeToImageSize();
            l.fillRect(0,0,1,1,0xffff0000);
            l.fillRect(1,0,1,1,0xff00ff00);
            l.fillRect(2,0,1,1,0x800000ff);
            global.before = "";
            for (var x = 0; x < 3; x++) before += l.getMainPixel(x,0) + " ";

            l.saveLayerImage("round.png", "png");

            global.dst = new Bitmap(3,1);
            dst.load("round.png");
            global.after = "";
            for (var x = 0; x < 3; x++) after += dst.getPixel(x,0) + " ";
        "#,
            "image round trip",
        )
        .expect("runs the test script");

    assert_eq!(
        engine.eval("before").expect("before"),
        engine.eval("after").expect("after"),
        "pixels differ after the round trip"
    );
}
