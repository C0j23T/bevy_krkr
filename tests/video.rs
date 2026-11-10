#![cfg(feature = "video-ffmpeg")]
mod common;

use bevy_krkr::{
    script::{Tjs2Engine, last_error},
    sys,
};
use std::{ffi::CString, time::Duration};

fn script(engine: &Tjs2Engine, source: &str) {
    engine
        .exec_script(source, "video test")
        .unwrap_or_else(|error| panic!("{source}\n{error}"));
}
fn pixel() -> [u8; 4] {
    let (mut width, mut height) = (0, 0);
    let id = unsafe { sys::krkr_scene_build(std::ptr::null_mut(), &mut width, &mut height) };
    assert_ne!(id, 0, "{}", last_error());
    let pixels = bevy_krkr::plugin::render::read_pixels(id).unwrap();
    pixels[((height / 2 * width + width / 2) * 4) as usize..][..4]
        .try_into()
        .unwrap()
}
fn tick() {
    assert_eq!(
        unsafe { sys::krkr_engine_tick() },
        sys::KRKR_OK,
        "{}",
        last_error()
    );
}

#[test]
fn video_commands_decode_into_the_gpu_scene() {
    common::initialize_renderer();
    bevy_krkr::plugin::video::initialize(std::env::var_os("KRKR_TEST_HARDWARE").is_some());
    let data = CString::new(format!("{}/tests/fixtures", env!("CARGO_MANIFEST_DIR"))).unwrap();
    assert_eq!(
        unsafe { sys::krkr_engine_init(data.as_ptr()) },
        sys::KRKR_OK,
        "{}",
        last_error()
    );
    let engine = Tjs2Engine::new().unwrap();
    script(
        &engine,
        r#"
        global.w = new Window(); w.setInnerSize(64,64); w.visible = true;
        global.base = new Layer(w,null); base.setImageSize(64,64); base.setSizeToImageSize();
        base.fillRect(0,0,64,64,0xff0000ff);
        global.v = new VideoOverlay(w); v.open("video.mp4");
        v.setBounds(0,0,64,64); v.visible = true; global.ended = false;
        v.onStatusChanged = function(status) { if (status == "stop") global.ended = true; };
        v.play();
        if (v.numberOfAudioStream != 1) throw "missing audio stream";
    "#,
    );
    assert!(
        engine
            .exec_script("v.playRate = 0;", "invalid video rate")
            .unwrap_err()
            .to_string()
            .contains("invalid video playback rate")
    );
    script(&engine, "global.bad = new VideoOverlay(w);");
    let error = engine
        .exec_script("bad.open('not-video.txt');", "invalid video input")
        .unwrap_err();
    assert!(
        !error.to_string().contains("Video backend failed"),
        "{error}"
    );
    script(&engine, "invalidate bad;");
    let mut color = [0; 4];
    for _ in 0..100 {
        tick();
        color = pixel();
        if color[0] > 240 && color[1] < 10 && color[2] < 10 {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        color[0] > 240 && color[1] < 10 && color[2] < 10,
        "{color:?}"
    );
    let mut pcm = vec![0.0; 2048];
    for _ in 0..50 {
        pcm.fill(0.0);
        tick();
        bevy_krkr::plugin::video::mix_audio(&mut pcm, 48000.0);
        if pcm.iter().any(|sample| sample.abs() > 0.01) {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        pcm.iter().any(|sample| sample.abs() > 0.01),
        "video audio is silent"
    );
    script(&engine, "v.audioVolume = 0;");
    pcm.fill(0.0);
    bevy_krkr::plugin::video::mix_audio(&mut pcm, 48000.0);
    assert!(pcm.iter().all(|sample| *sample == 0.0));
    script(&engine, "v.audioVolume = 100000; v.audioBalance = 100000;");
    pcm.fill(0.0);
    bevy_krkr::plugin::video::mix_audio(&mut pcm, 48000.0);
    assert!(pcm.as_chunks::<2>().0.iter().all(|sample| sample[0] == 0.0));
    assert!(
        pcm.as_chunks::<2>()
            .0
            .iter()
            .any(|sample| sample[1].abs() > 0.01)
    );
    script(
        &engine,
        "v.audioBalance = 0; v.playRate = 2; if (v.playRate != 2) throw 'rate failed'; v.playRate = 1;",
    );
    script(&engine, "v.pause(); global.paused = v.frame;");
    pcm.fill(0.0);
    bevy_krkr::plugin::video::mix_audio(&mut pcm, 48000.0);
    assert!(pcm.iter().all(|sample| *sample == 0.0));
    std::thread::sleep(Duration::from_millis(130));
    tick();
    script(
        &engine,
        "v.brightness = 0.2; v.contrast = 0; v.hue = 180; v.saturation = 0; v.mixingMovieAlpha = 0; v.mixingMovieBGColor = 0xffffffff; v.setMixingLayer(base); v.resetMixingLayer();",
    );
    assert!(pixel()[0] > 240, "unsupported controls changed video");
    script(
        &engine,
        "if (v.mixingMovieAlpha != 1 || (v.mixingMovieBGColor & 0xffffff) != 0) throw 'mixer defaults';",
    );
    script(
        &engine,
        "if (v.frame != paused) throw 'frame advanced while paused'; v.position = 500;",
    );
    tick();
    let mut sought = [0; 4];
    for _ in 0..40 {
        tick();
        sought = pixel();
        if sought[1] > 240 && sought[0] < 10 {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        sought[1] > 240 && sought[0] < 10,
        "seek did not change the frame: {sought:?}"
    );
    script(
        &engine,
        "v.stop(); if (!v.visible) throw 'stop changed visibility';",
    );
    assert_eq!(pixel(), [0, 0, 255, 255], "stop left video visible");
    for _ in 0..10 {
        tick();
        std::thread::sleep(Duration::from_millis(5));
    }
    script(
        &engine,
        "v.visible = false; v.visible = true; v.position = 500;",
    );
    tick();
    assert_eq!(pixel(), [0, 0, 255, 255], "stopped preview became visible");
    script(&engine, "v.visible = false; v.play(); v.pause();");
    assert_eq!(pixel(), [0, 0, 255, 255], "play ignored visibility");
    script(&engine, "v.visible = true;");
    for _ in 0..40 {
        tick();
        if pixel()[1] > 240 {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(pixel()[1] > 240, "play did not restore paused picture");
    script(
        &engine,
        "if (v.position < 400) throw 'seek failed'; v.visible = false;",
    );
    assert_eq!(pixel(), [0, 0, 255, 255]);
    script(&engine, "v.visible = true; v.position = 900; v.play();");
    for _ in 0..100 {
        tick();
        if engine.eval("ended").unwrap() == "1" {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(engine.eval("ended").unwrap(), "1");
    script(&engine, "v.loop = true; v.position = 900; v.play();");
    for _ in 0..30 {
        tick();
        std::thread::sleep(Duration::from_millis(20));
    }
    let position: f64 = engine.eval("v.position").unwrap().parse().unwrap();
    assert!(position < 900.0, "loop did not rewind: {position}");
    script(
        &engine,
        "global.v2 = new VideoOverlay(w); v2.open('video.mp4'); v2.play(); v2.pause(); v2.close(); invalidate v2;",
    );
    script(&engine, "v.loop = false; v.stop(); v.close();");
    assert_eq!(pixel(), [0, 0, 255, 255], "cached video remained visible");
    script(
        &engine,
        "v.open('video.mp4'); v.setBounds(0,0,64,64); v.play();",
    );
    for _ in 0..40 {
        tick();
        if pixel()[0] > 240 {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(pixel()[0] > 240, "cached video failed to reattach");
    script(
        &engine,
        "v.close(); v.open('multistream.mp4'); v.loop = false; v.pause(); if (v.numberOfAudioStream != 2 || v.numberOfVideoStream != 2) throw 'stream counts'; v.enabledAudioStream = 1; v.enabledVideoStream = 1; if (v.enabledAudioStream != 1 || v.enabledVideoStream != 1) throw 'selected streams';",
    );
    let mut selected = [0; 4];
    for _ in 0..100 {
        tick();
        selected = pixel();
        if selected[2] > 240 && selected[0] < 10 {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        selected[2] > 240 && selected[0] < 10,
        "stream switch: {selected:?}"
    );
    assert!(
        engine
            .exec_script("v.enabledVideoStream = 2;", "invalid stream")
            .is_err()
    );
    script(
        &engine,
        "if (v.enabledVideoStream != 1) throw 'invalid switch changed state'; v.enabledAudioStream = 0; v.enabledVideoStream = 0; v.play();",
    );
    let mut selected = [0; 4];
    for _ in 0..100 {
        tick();
        selected = pixel();
        if selected[0] > 240 && selected[2] < 10 {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        selected[0] > 240 && selected[2] < 10,
        "switch back: {selected:?}"
    );
    script(
        &engine,
        "v.setSegmentLoop(5,-1); v.position = 900; v.play();",
    );
    for _ in 0..30 {
        tick();
        std::thread::sleep(Duration::from_millis(10));
    }
    let position: f64 = engine.eval("v.position").unwrap().parse().unwrap();
    assert!(
        (500.0..900.0).contains(&position),
        "EOF loop start: {position}"
    );
    script(
        &engine,
        "v.loop = false; v.position = 900; v.play(); global.ended = false;",
    );
    for _ in 0..100 {
        tick();
        if engine.eval("ended").unwrap() == "1" {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(engine.eval("ended").unwrap(), "1");
    // Destroy the video's window while retaining the video object and its cache.
    script(
        &engine,
        "global.other = new Window(); other.setInnerSize(64,64); global.otherBase = new Layer(other,null); otherBase.setImageSize(64,64); otherBase.setSizeToImageSize(); otherBase.fillRect(0,0,64,64,0xffffff00); other.visible = true; invalidate w;",
    );
    tick();
    assert_eq!(
        pixel(),
        [255, 255, 0, 255],
        "video leaked into another window"
    );
    script(&engine, "invalidate v;");
    tick();
    assert_eq!(pixel(), [255, 255, 0, 255]);
    drop(engine);
    unsafe {
        sys::krkr_engine_shutdown();
    }
    assert_eq!(
        unsafe { sys::krkr_engine_init(data.as_ptr()) },
        sys::KRKR_OK,
        "{}",
        last_error()
    );
    let engine = Tjs2Engine::new().unwrap();
    script(
        &engine,
        "global.w = new Window(); w.setInnerSize(64,64); w.visible = true; global.base = new Layer(w,null); base.setImageSize(64,64); base.setSizeToImageSize(); global.v = new VideoOverlay(w); v.open('video.mp4'); v.setBounds(0,0,64,64); v.visible = true; v.play();",
    );
    for _ in 0..10 {
        tick();
        std::thread::sleep(Duration::from_millis(10));
    }
    drop(engine);
    unsafe {
        sys::krkr_engine_shutdown();
    }
    assert_eq!(bevy_krkr::plugin::render::texture_usage().0, 0);
}
