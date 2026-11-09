//! Images reach the engine and leave it again through `krkr_shim`: PNG
//! losslessly via `LoadPNG.cpp`, JPEG lossily via `LoadJPEG.cpp`. The engine is
//! initialised once per process, so this is one test.

mod common;

use std::ffi::CString;
use std::path::PathBuf;

use bevy_krkr::script::{Tjs2Engine, last_error};
use bevy_krkr::sys;

/// `getPixel` packs a pixel as `r + g << 8 + b << 16`.
fn channels(value: &str) -> [u32; 3] {
    let value: u32 = value.parse().expect("a pixel value");
    [value & 0xff, (value >> 8) & 0xff, (value >> 16) & 0xff]
}

#[test]
fn a_layer_survives_an_image_round_trip() {
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

            // One pixel per color, for the lossless check.
            global.small = new Layer(w,null); small.setImageSize(3,1); small.setSizeToImageSize();
            small.fillRect(0,0,1,1,0xffff0000);
            small.fillRect(1,0,1,1,0xff00ff00);
            small.fillRect(2,0,1,1,0x800000ff);
            global.before = "";
            for (var x = 0; x < 3; x++) before += small.getMainPixel(x,0) + " ";
            small.saveLayerImage("lossless.png", "png");

            // Solid quadrants, so a JPEG's chroma subsampling only shows on
            // their edges and the middles stay comparable. Only one layer may be
            // a child of the window, so this hangs off the first.
            global.quad = new Layer(w,small); quad.setImageSize(16,16); quad.setSizeToImageSize();
            quad.fillRect(0,0,8,8,0xffff0000);
            quad.fillRect(8,0,8,8,0xff00ff00);
            quad.fillRect(0,8,8,8,0xff0000ff);
            quad.fillRect(8,8,8,8,0xffffffff);
            global.corners = "";
            var spots = [[2,2],[13,2],[2,13],[13,13]];
            for (var i = 0; i < spots.length; i++)
                corners += quad.getMainPixel(spots[i][0], spots[i][1]) + " ";
            quad.saveLayerImage("lossy.jpg", "jpg");
        "#,
            "image round trip",
        )
        .expect("runs the test script");

    engine
        .exec_script(
            r#"
            global.png = new Bitmap(3,1); png.load("lossless.png");
            global.after = "";
            for (var x = 0; x < 3; x++) after += png.getPixel(x,0) + " ";

            global.jpg = new Bitmap(16,16); jpg.load("lossy.jpg");
            global.jpgCorners = "";
            var spots = [[2,2],[13,2],[2,13],[13,13]];
            for (var i = 0; i < spots.length; i++)
                jpgCorners += jpg.getPixel(spots[i][0], spots[i][1]) + " ";
        "#,
            "image reload",
        )
        .expect("runs the reload script");

    assert_eq!(
        engine.eval("after").expect("after"),
        engine.eval("before").expect("before"),
        "PNG is lossless"
    );

    let before: Vec<[u32; 3]> = engine
        .eval("corners")
        .expect("corners")
        .split_whitespace()
        .map(channels)
        .collect();
    let after: Vec<[u32; 3]> = engine
        .eval("jpgCorners")
        .expect("jpg corners")
        .split_whitespace()
        .map(channels)
        .collect();
    assert_eq!(before.len(), 4);
    for (index, (after, before)) in after.iter().zip(&before).enumerate() {
        for (channel, (&after, &before)) in after.iter().zip(before).enumerate() {
            assert!(
                after.abs_diff(before) <= 16,
                "JPEG corner {index} channel {channel} is {after}, expected {before}"
            );
        }
    }
}
