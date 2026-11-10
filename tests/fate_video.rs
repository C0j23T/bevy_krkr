#![cfg(feature = "video-ffmpeg")]
mod common;

use bevy_krkr::{
    script::{Tjs2Engine, last_error},
    sys,
};
use std::{
    ffi::CString,
    path::PathBuf,
    time::{Duration, Instant},
};

fn script(engine: &Tjs2Engine, source: &str) {
    engine.exec_script(source, "Fate movie test").unwrap();
}
fn tick() {
    assert_eq!(
        unsafe { sys::krkr_engine_tick() },
        sys::KRKR_OK,
        "{}",
        last_error()
    );
    std::thread::sleep(Duration::from_millis(10));
}
fn capture() -> Vec<u8> {
    let (mut width, mut height) = (0, 0);
    let id = unsafe { sys::krkr_scene_build(std::ptr::null_mut(), &mut width, &mut height) };
    assert_eq!((width, height), (800, 600));
    bevy_krkr::plugin::render::read_pixels(id).unwrap()
}

#[test]
#[ignore = "requires the local Fate hollow ataraxia archives and a hardware decoder"]
fn fate_movies_play_from_archives() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test_games/Fate hollow ataraxia");
    assert!(root.join("data.xp3").exists());
    common::initialize_renderer();
    bevy_krkr::video::initialize(true);
    // Keep engine logs/configuration away from the game's savedata directory.
    // Only load Movie.tjs, never startup.tjs or the game's save/load handlers.
    let project = std::env::temp_dir().join(format!("krkr-fate-movie-{}", std::process::id()));
    std::fs::create_dir(&project).unwrap();
    let data = CString::new(project.to_str().unwrap()).unwrap();
    assert_eq!(
        unsafe { sys::krkr_engine_init(data.as_ptr()) },
        sys::KRKR_OK,
        "{}",
        last_error()
    );
    let engine = Tjs2Engine::new().unwrap();
    script(
        &engine,
        &format!(
            r#"
        Storages.addAutoPath({:?});
        Storages.addAutoPath({:?});
        Scripts.execStorage({:?});
        function dm(message) {{ Debug.message(message); }}
        class MovieWindow extends Window {{
            var stopped = 0;
            function MovieWindow() {{ super.Window(); }}
            function onMovieStop(id) {{ stopped++; }}
            function onMoviePeriod(id, type) {{}}
        }}
        global.w = new MovieWindow(); w.setInnerSize(800,600); w.visible = true;
        global.base = new Layer(w,null); base.setImageSize(800,600); base.setSizeToImageSize();
        base.fillRect(0,0,800,600,0xff000000);
    "#,
            format!("{}>video/", root.join("data.xp3").display()),
            format!("{}>", root.join("patch.xp3").display()),
            format!("{}>system/movie.tjs", root.join("data.xp3").display())
        ),
    );
    for name in ["fd_op01.mpg", "fd_op02.mpg", "fd_vop01.mpg", "fd_vop02.mpg"] {
        script(
            &engine,
            &format!(
                r#"
            global.movie = new Movie(w); movie.open({name:?});
            if (!movie.opened) throw "game Movie.open failed";
            if (movie.originalWidth != 800 || movie.originalHeight != 600) throw "video dimensions";
            movie.setBounds(0,0,800,600); movie.visible = true; movie.play();
            movie.position = 10000;
        "#
            ),
        );
        let mut heard = false;
        let mut visible = false;
        let mut frame = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            tick();
            let mut pcm = [0.0; 2048];
            bevy_krkr::video::mix_audio(&mut pcm, 48000.0);
            heard |= pcm.iter().any(|v| v.abs() > 0.001);
            if !visible {
                frame = capture();
                visible = frame
                    .chunks_exact(4)
                    .filter(|p| p[..3].iter().any(|v| *v > 20))
                    .take(1000)
                    .count()
                    == 1000;
            }
            if heard && visible {
                break;
            }
        }
        assert!(visible, "{name}: no video picture");
        assert!(heard, "{name}: no video audio");
        script(&engine, "movie.pause();");
        let paused = capture();
        for _ in 0..5 {
            tick();
        }
        assert_eq!(capture(), paused, "{name}: paused picture changed");
        script(&engine, "movie.stop();");
        assert!(
            capture()
                .chunks_exact(4)
                .all(|pixel| pixel[..3] == [0, 0, 0]),
            "{name}: stopped picture remained visible"
        );
        script(
            &engine,
            &format!(
                "movie.open({name:?}); movie.visible = true; movie.play(); movie.position = 10000;"
            ),
        );
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut resumed = false;
        while Instant::now() < deadline {
            tick();
            resumed = capture()
                .chunks_exact(4)
                .filter(|p| p[..3].iter().any(|v| *v > 20))
                .take(1000)
                .count()
                == 1000;
            if resumed {
                break;
            }
        }
        assert!(resumed, "{name}: cached reopen did not restore video");
        if let Some(directory) = std::env::var_os("KRKR_VIDEO_CAPTURE") {
            let directory = PathBuf::from(directory);
            std::fs::create_dir_all(&directory).unwrap();
            let mut ppm = b"P6\n800 600\n255\n".to_vec();
            for pixel in frame.chunks_exact(4) {
                ppm.extend_from_slice(&pixel[..3]);
            }
            std::fs::write(directory.join(format!("{name}.ppm")), ppm).unwrap();
        }
        let duration: f64 = engine.eval("movie.totalTime").unwrap().parse().unwrap();
        let stopped: u32 = engine.eval("w.stopped").unwrap().parse().unwrap();
        script(
            &engine,
            &format!(
                "movie.pause(); movie.position = {}; movie.resume();",
                duration - 1200.0
            ),
        );
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            tick();
            if engine.eval("w.stopped").unwrap().parse::<u32>().unwrap() > stopped {
                break;
            }
        }
        assert!(
            engine.eval("w.stopped").unwrap().parse::<u32>().unwrap() > stopped,
            "{name}: no game onMovieStop callback"
        );
        println!(
            "{name}: duration={duration:.0} ms; GPU picture, PCM, pause, stop hiding, cached reopen, seek and game end callback passed"
        );
        script(&engine, "invalidate movie;");
    }
    script(&engine, "invalidate w;");
    drop(engine);
    unsafe {
        sys::krkr_engine_shutdown();
    }
    assert_eq!(bevy_krkr::plugin::render::texture_usage().0, 0);
    std::fs::remove_dir_all(project).unwrap();
}
