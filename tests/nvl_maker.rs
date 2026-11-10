mod common;
use bevy_krkr::{
    script::{Tjs2Engine, last_error},
    sys,
};
use std::{ffi::CString, path::Path, time::Duration};
struct Session;
impl Drop for Session {
    fn drop(&mut self) {
        unsafe { sys::krkr_engine_shutdown() };
    }
}
fn copy_game(source: &Path, target: &Path) {
    std::fs::create_dir_all(target).unwrap();
    for item in std::fs::read_dir(source).unwrap() {
        let item = item.unwrap();
        if item.file_type().unwrap().is_dir() {
            copy_game(&item.path(), &target.join(item.file_name()));
        } else {
            std::fs::copy(item.path(), target.join(item.file_name())).unwrap();
        }
    }
}
fn pump(engine: &Tjs2Engine, condition: &str, frames: usize) -> f32 {
    let mut peak = 0.0_f32;
    for _frame in 0..frames {
        assert_eq!(
            unsafe { sys::krkr_engine_tick() },
            sys::KRKR_OK,
            "{}",
            last_error()
        );
        let (mut width, mut height) = (0, 0);
        assert_ne!(
            unsafe { sys::krkr_scene_build(std::ptr::null_mut(), &mut width, &mut height) },
            0,
            "{}",
            last_error()
        );
        let mut pcm = [0.0; 960];
        unsafe { sys::krkr_audio_mix(pcm.as_mut_ptr(), 480, 48000.0) };
        peak = pcm.iter().fold(peak, |p, s| p.max(s.abs()));
        if engine.eval(condition).unwrap() == "1" && _frame >= 2 {
            return peak;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!(
        "Timed out: {condition}; position: {:?}",
        engine.eval("kag.conductor.curStorage+':'+kag.conductor.curLine")
    );
}
fn capture(name: &str) {
    let (mut w, mut h) = (0, 0);
    let id = unsafe { sys::krkr_scene_build(std::ptr::null_mut(), &mut w, &mut h) };
    assert_ne!(id, 0, "{}", last_error());
    let pixels = bevy_krkr::plugin::render::read_pixels(id).unwrap();
    assert!(
        pixels.chunks_exact(4).any(|p| p[0] != p[1] || p[1] != p[2]),
        "{name} produced no colored UI"
    );
    if let Some(path) = std::env::var_os("NVL_MAKER_CAPTURE") {
        let path = Path::new(&path);
        std::fs::create_dir_all(path).unwrap();
        let mut ppm = format!("P6\n{w} {h}\n255\n").into_bytes();
        for p in pixels.chunks_exact(4) {
            ppm.extend_from_slice(&p[..3]);
        }
        std::fs::write(path.join(format!("{name}.ppm")), ppm).unwrap();
    }
}
fn click(x: i32, y: i32) {
    unsafe {
        sys::krkr_window_mouse_move(x, y);
        sys::krkr_window_mouse_down(x, y, 0);
        sys::krkr_window_mouse_up(x, y, 0);
    }
}
#[test]
#[ignore = "requires the NVL Maker 3.87 template in NVL_MAKER_DATA"]
fn exported_template_gameplay() {
    let source = std::env::var_os("NVL_MAKER_DATA").expect("NVL_MAKER_DATA");
    let temp = std::env::temp_dir().join(format!("krkr-nvl-{}", std::process::id()));
    let packed = Path::new(&source).is_file();
    if packed {
        std::fs::create_dir_all(&temp).unwrap();
        std::fs::copy(&source, temp.join("data.xp3")).unwrap();
    } else {
        copy_game(Path::new(&source), &temp);
    }
    common::initialize_renderer();
    let game_path = if packed {
        temp.join("data.xp3")
    } else {
        temp.clone()
    };
    let path = CString::new(game_path.to_string_lossy().as_bytes()).unwrap();
    assert_eq!(
        unsafe { sys::krkr_engine_init(path.as_ptr()) },
        sys::KRKR_OK,
        "{}",
        last_error()
    );
    let session = Session;
    let engine = Tjs2Engine::new().unwrap();
    engine.exec_startup().unwrap();
    pump(
        &engine,
        "kag.conductor.curStorage == 'title.ks' && kag.conductor.curLabel == '*title' && kag.conductor.curLine >= 62",
        800,
    );
    capture("title");
    engine
        .exec_script("f.__compat_save=731;", "save marker")
        .unwrap();
    click(450, 350);
    let peak = pump(
        &engine,
        "kag.conductor.curStorage == 'prologue.ks' && kag.clickWaiting && kag.inStable && !kag.transShowing",
        800,
    );
    assert!(peak > 0.001, "template BGM produced no PCM");
    capture("dialogue");
    eprintln!(
        "dialogue: {}",
        engine.eval("kag.conductor.curLine").unwrap()
    );
    assert_eq!(engine.eval("kag.saveBookMark(0)").unwrap(), "1");
    assert!(std::fs::read_dir(temp.join("savedata")).unwrap().count() > 0);
    engine
        .exec_script("f.__compat_save=0;", "change saved variable")
        .unwrap();
    assert_eq!(engine.eval("kag.loadBookMark(0)").unwrap(), "1");
    pump(
        &engine,
        "kag.conductor.curStorage == 'prologue.ks' && kag.clickWaiting && kag.inStable && !kag.transShowing",
        800,
    );
    assert_eq!(engine.eval("f.__compat_save").unwrap(), "731");
    capture("loaded");
    click(700, 650);
    pump(&engine, "kag.clickWaiting && kag.inStable", 400);
    click(700, 650);
    pump(
        &engine,
        "kag.clickWaiting && kag.inStable && kag.conductor.curLine > 12",
        400,
    );
    assert_eq!(engine.eval("kag.historyLayer.dataPos > 0").unwrap(), "1");
    engine.exec_script("kag.showHistory();", "history").unwrap();
    assert_eq!(engine.eval("kag.historyLayer.visible").unwrap(), "1");
    capture("history");
    engine
        .exec_script("kag.hideHistory();", "hide history")
        .unwrap();
    let line = engine.eval("kag.conductor.curLine").unwrap();
    click(965, 615);
    pump(&engine, "kag.autoMode", 100);
    pump(
        &engine,
        &format!("kag.conductor.curLine > {line} && kag.clickWaiting"),
        800,
    );
    click(965, 615);
    pump(&engine, "!kag.autoMode", 100);
    engine
        .exec_script("kag.allskip=true; kag.skipToStop();", "skip dialogue")
        .unwrap();
    pump(
        &engine,
        "kag.conductor.curLabel == '*start' && kag.conductor.curLine > 80 && kag.conductor.status == kag.conductor.mStop",
        1000,
    );
    capture("name-entry");
    eprintln!(
        "name entry line: {}",
        engine.eval("kag.conductor.curLine").unwrap()
    );
    click(375, 655);
    pump(
        &engine,
        "kag.conductor.curLabel == '*输入完毕' && kag.clickWaiting && kag.inStable",
        500,
    );
    engine
        .exec_script("kag.allskip=true; kag.skipToStop();", "skip to choices")
        .unwrap();
    pump(
        &engine,
        "kag.conductor.curLine >= 105 && kag.conductor.status == kag.conductor.mStop",
        1000,
    );
    capture("choices");
    eprintln!(
        "choice line: {}",
        engine.eval("kag.conductor.curLine").unwrap()
    );
    click(500, 220);
    pump(
        &engine,
        "kag.conductor.curLabel == '*选择A' && kag.clickWaiting && kag.inStable",
        800,
    );
    capture("choice-a");
    engine
        .exec_script("kag.goToStart();", "return to title")
        .unwrap();
    pump(
        &engine,
        "kag.conductor.curStorage == 'title.ks' && kag.conductor.curLabel == '*title' && kag.conductor.curLine >= 62",
        800,
    );
    capture("returned-title");
    engine
        .exec_script("kag.askOnClose=false; kag.close();", "close game")
        .unwrap();
    for _ in 0..20 {
        if unsafe { sys::krkr_engine_exit_requested() } != 0 {
            break;
        }
        unsafe { sys::krkr_engine_tick() };
    }
    assert_ne!(unsafe { sys::krkr_engine_exit_requested() }, 0);

    drop(engine);
    drop(session);
    assert_eq!(
        unsafe { sys::krkr_engine_init(path.as_ptr()) },
        sys::KRKR_OK,
        "{}",
        last_error()
    );
    let session = Session;
    let engine = Tjs2Engine::new().unwrap();
    engine.exec_startup().unwrap();
    pump(
        &engine,
        "kag.conductor.curStorage == 'title.ks' && kag.conductor.curLabel == '*title' && kag.conductor.curLine >= 62",
        800,
    );
    assert_eq!(engine.eval("kag.loadBookMark(0)").unwrap(), "1");
    pump(
        &engine,
        "kag.conductor.curStorage == 'prologue.ks' && kag.clickWaiting && kag.inStable && !kag.transShowing",
        800,
    );
    capture("restarted-load");
    engine
        .exec_script("kag.bgm.stop();", "isolate sound buffers")
        .unwrap();
    // The template has no recorded voice; exercise its voice buffer with the supplied WAV.
    for buffer in [0, 1] {
        engine
            .exec_script(
                &format!("kag.se[{buffer}].play(%[storage:'kaizan-system02.wav',loop:false]);"),
                "SE/voice buffer",
            )
            .unwrap();
        let peak = pump(&engine, &format!("kag.se[{buffer}].status == 'stop'"), 300);
        assert!(peak > 0.001, "SE/voice buffer {buffer} produced no PCM");
    }
    assert_eq!(engine.eval("f.__compat_save").unwrap(), "731");
    if std::env::var_os("NVL_MAKER_VIDEO").is_some() {
        #[cfg(feature = "video-ffmpeg")]
        bevy_krkr::plugin::video::initialize(false);
        std::fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/layer-alpha.mkv"),
            temp.join("layer-alpha.mkv"),
        )
        .unwrap();
        let movie_path = temp.join("layer-alpha.mkv").to_string_lossy().to_string();
        engine
            .exec_script(
                &format!(
                    r#"
            global.movieA=kag.fore.layers[6]; global.movieB=kag.fore.layers[7];
            movieA.startMovie({movie_path:?},true,false);
            global.controller=movieA.movie; controller.entry(movieB); movieB.movie=controller;
            movieA.visible=true; movieB.visible=true;
        "#
                ),
                "template Layer movie",
            )
            .unwrap();
        pump(
            &engine,
            "movieA.movie === void && movieB.movie === void && kag.movieLayers.count == 0",
            300,
        );
        for layer in ["movieA", "movieB"] {
            let rgb = engine
                .eval(&format!("{layer}._image.getMainPixel(0,0)"))
                .unwrap()
                .parse::<u32>()
                .unwrap();
            let alpha = engine
                .eval(&format!("{layer}._image.getMaskPixel(0,0)"))
                .unwrap()
                .parse::<u32>()
                .unwrap();
            assert!(
                (rgb & 255) > 245 && ((rgb >> 8) & 255) < 5 && ((rgb >> 16) & 255) < 5,
                "{rgb:x}"
            );
            assert!(alpha.abs_diff(128) < 5, "{alpha}");
        }
        capture("layer-movie");
    }
    drop(engine);
    drop(session);
    std::fs::remove_dir_all(temp).unwrap();
}
