//! GPU window output and native input delivery.

mod common;

use std::ffi::CString;
use std::path::PathBuf;

use bevy_krkr::script::{last_error, Tjs2Engine};
use bevy_krkr::sys;

fn data_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/game")
}

fn pixel(x: u32, y: u32) -> (u8,u8,u8,u8) {
    let (mut width,mut height) = (0,0);
    let texture = unsafe { sys::krkr_scene_build(std::ptr::null_mut(),&mut width,&mut height) };
    assert_ne!(texture,0);
    assert!(x < width as u32 && y < height as u32);
    let pixels = bevy_krkr::plugin::render::read_pixels(texture).unwrap();
    let i = (y*width as u32+x) as usize*4;
    (pixels[i],pixels[i+1],pixels[i+2],pixels[i+3])
}

#[test]
fn window_is_rendered_and_takes_input() {
    common::initialize_renderer();
    let path = data_path();
    let c_path = CString::new(path.to_string_lossy().as_bytes()).expect("no NUL in the path");
    // SAFETY: called on the test thread, one session at a time.
    let code = unsafe {
        sys::krkr_engine_set_screen_size(800, 600);
        sys::krkr_engine_init(c_path.as_ptr())
    };
    assert_eq!(code, sys::KRKR_OK, "krkr_engine_init: {}", last_error());
    let engine = Tjs2Engine::new().expect("Tjs2Engine::new");

    // No window yet.
    let (mut width, mut height) = (0, 0);
    // SAFETY: the engine is running; out-parameters are valid.
    let ptr = unsafe { sys::krkr_scene_build(std::ptr::null_mut(), &mut width, &mut height) };
    assert!(ptr == 0, "a window appeared before one was created");

    let script = r#"
        global.w = new Window();
        w.setInnerSize(320, 240);
        global.wl = new Layer(w, null);
        wl.setImageSize(320, 240);
        wl.setSizeToImageSize();
        wl.fillRect(0, 0, 320, 240, 0xff204080);  // ARGB
        wl.fillRect(0, 0, 10, 10, 0xffff0000);
        // Handlers the host's input has to reach.
        global.clicks = 0;
        w.onMouseDown = function(x, y, button) { global.clicks += 1; };
        global.moves = "";
        w.onMouseMove = function(x, y) { global.moves += x + "," + y + " "; };
        global.keys = "";
        w.onKeyDown = function(key, shift) { global.keys += "down" + key + "@" + shift + " "; };
        w.onKeyUp = function(key, shift) { global.keys += "up" + key + "@" + shift + " "; };
        w.visible = true;
        w.update();
    "#;
    engine
        .exec_script(script, "window test")
        .expect("window script");

    // The window is shown, and shows what was drawn into it: Kirikiroid2 renders
    // RGBA, so the ARGB 0xffff0000 fill comes out as opaque red.
    assert_eq!(pixel(5, 5), (255, 0, 0, 255), "top-left block");
    assert_eq!(pixel(300, 200), (0x20, 0x40, 0x80, 255), "background");

    // Host input reaches the window's TJS handlers.
    let (mut width, mut height) = (0, 0);
    // SAFETY: the engine is running; out-parameters are valid.
    let ptr = unsafe { sys::krkr_scene_build(std::ptr::null_mut(), &mut width, &mut height) };
    assert!(ptr != 0);
    assert_eq!((width, height), (320, 240), "window size");

    unsafe { sys::krkr_window_mouse_down(100, 100, 0) };
    unsafe { sys::krkr_window_mouse_up(100, 100, 0) };
    unsafe { sys::krkr_window_mouse_move(123, 45) };
    // Enter (13) with the shift bit set, then its release.
    unsafe { sys::krkr_window_key_down(13, 0x1) };
    unsafe { sys::krkr_window_key_up(13, 0x1) };
    // Input events are delivered by the engine's event queue, i.e. on the next
    // tick.
    unsafe { sys::krkr_engine_tick() };
    assert_eq!(
        engine.eval("global.clicks").unwrap(),
        "1",
        "onMouseDown ran"
    );
    assert_eq!(
        engine.eval("global.moves").unwrap(),
        "123,45 ",
        "onMouseMove ran with the posted position"
    );
    assert_eq!(
        engine.eval("global.keys").unwrap(),
        "down13@1 up13@1 ",
        "onKeyDown/onKeyUp ran with the posted key and shift state"
    );

    // Hiding it (or closing it) stops the host from showing it.
    engine
        .exec_script("w.visible = false;", "window test")
        .unwrap();
    // SAFETY: the engine is running; out-parameters are valid.
    let ptr = unsafe { sys::krkr_scene_build(std::ptr::null_mut(), &mut width, &mut height) };
    assert!(ptr == 0, "a hidden window is still shown");

    drop(engine);
    // SAFETY: every handle created above is released.
    unsafe { sys::krkr_engine_shutdown() };
}
