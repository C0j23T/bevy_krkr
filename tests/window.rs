//! The Kirikiri `Window` class, rendered off-screen for the host.
//!
//! KAG's own UI (`KAGWindow` in a game's `mainwindow.tjs`) is built on
//! `Window`; the port backs it with `environ/bevy/BevyWindowLayer.cpp`, which
//! renders the window into a texture the host uploads, and forwards host input
//! to the window's TJS handlers. This test drives that path directly: create a
//! window, paint known colours into it, read the snapshot back, and post a
//! click to a handler.

use std::ffi::CString;
use std::path::PathBuf;

use bevy_krkr::script::{last_error, Tjs2Engine};
use bevy_krkr::sys;

fn data_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/game")
}

/// Colour of one pixel of the last snapshot.
fn pixel(x: u32, y: u32) -> (u8, u8, u8, u8) {
    let (mut width, mut height, mut revision) = (0u32, 0u32, 0u64);
    // SAFETY: an instance is running for the whole test; the buffer stays valid
    // until the next engine call, which is why it is copied immediately.
    let ptr = unsafe { sys::krkr_window_snapshot(&mut width, &mut height, &mut revision) };
    assert!(!ptr.is_null(), "no window snapshot");
    assert!(
        x < width && y < height,
        "({x}, {y}) outside {width}x{height}"
    );
    // SAFETY: the C ABI returns width*height*4 readable bytes.
    let pixels = unsafe { std::slice::from_raw_parts(ptr, (width * height * 4) as usize) };
    let i = ((y * width + x) * 4) as usize;
    (pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3])
}

#[test]
fn window_is_rendered_and_takes_input() {
    let path = data_path();
    let c_path = CString::new(path.to_string_lossy().as_bytes()).expect("no NUL in the path");
    // SAFETY: called on the test thread, one session at a time.
    let code = unsafe {
        sys::krkr_engine_set_screen_size(800, 600);
        sys::krkr_engine_init(c_path.as_ptr())
    };
    assert_eq!(code, sys::KRKR_OK, "krkr_engine_init: {}", last_error());
    let engine = Tjs2Engine::new().expect("Tjs2Engine::new");

    // No window yet: nothing for the host to show.
    let (mut width, mut height) = (0u32, 0u32);
    // SAFETY: the engine is running; out-parameters are valid.
    let ptr = unsafe { sys::krkr_window_snapshot(&mut width, &mut height, std::ptr::null_mut()) };
    assert!(ptr.is_null(), "a window appeared before one was created");

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
    let (mut width, mut height) = (0u32, 0u32);
    // SAFETY: the engine is running; out-parameters are valid.
    let ptr = unsafe { sys::krkr_window_snapshot(&mut width, &mut height, std::ptr::null_mut()) };
    assert!(!ptr.is_null());
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
    let ptr = unsafe { sys::krkr_window_snapshot(&mut width, &mut height, std::ptr::null_mut()) };
    assert!(ptr.is_null(), "a hidden window is still shown");

    drop(engine);
    // SAFETY: every handle created above is released.
    unsafe { sys::krkr_engine_shutdown() };
}
