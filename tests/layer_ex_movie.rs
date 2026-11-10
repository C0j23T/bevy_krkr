mod common;
#[cfg(feature = "video-ffmpeg")]
use bevy_krkr::script::last_error;
use bevy_krkr::{script::Tjs2Engine, sys};
use std::ffi::CString;
#[cfg(feature = "video-ffmpeg")]
use std::time::Duration;
#[cfg(feature = "video-ffmpeg")]
fn tick() {
    assert_eq!(
        unsafe { sys::krkr_engine_tick() },
        sys::KRKR_OK,
        "{}",
        last_error()
    );
    std::thread::sleep(Duration::from_millis(10));
}
#[test]
fn layer_movies() {
    common::initialize_renderer();
    #[cfg(feature = "video-ffmpeg")]
    bevy_krkr::plugin::video::initialize(false);
    let path = CString::new(format!("{}/tests/fixtures", env!("CARGO_MANIFEST_DIR"))).unwrap();
    assert_eq!(
        unsafe { sys::krkr_engine_init(path.as_ptr()) },
        sys::KRKR_OK
    );
    let engine = Tjs2Engine::new().unwrap();
    let run = |s: &str| engine.exec_script(s, "Layer movie").unwrap();
    run(r#"
        Plugins.link("layerExMovie.dll"); Plugins.link("layerExMovie.dll");
        global.w=new Window(); w.setInnerSize(128,128); w.visible=true;
        global.base=new Layer(w,null); base.setImageSize(128,128); base.setSizeToImageSize();
        base.fillRect(0,0,128,128,0xff0000ff);
        global.m=new Layer(w,base); m.visible=true; m.setPos(8,8);
    "#);
    #[cfg(not(feature = "video-ffmpeg"))]
    {
        let error = engine
            .exec_script("m.openMovie('video.mp4',false);", "missing backend")
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("video backend is not initialized"),
            "{error}"
        );
        assert_eq!(engine.eval("m.isPlayingMovie()").unwrap(), "0");
    }
    #[cfg(feature = "video-ffmpeg")]
    {
        run(r#"
            global.started=0; global.updated=0; global.stopped=0;
            m.onStartMovie=function(){global.started++;};
            global.other=new Layer(w,base); other.visible=true; other.setPos(72,8);
            m.onUpdateMovie=function(){global.updated++; other.assignImages(m); other.setImageSize(m.imageWidth,m.imageHeight); other.setSizeToImageSize();};
            m.onStopMovie=function(){global.stopped++;};
            m.openMovie('layer-alpha.mkv',true); m.startMovie(false);
        "#);
        for _ in 0..150 {
            tick();
            if engine.eval("stopped").unwrap() == "1" {
                break;
            }
        }
        assert_eq!(
            engine
                .eval("started+':'+stopped+':'+m.isPlayingMovie()")
                .unwrap(),
            "1:1:0"
        );
        assert!(engine.eval("updated > 0").unwrap() == "1");
        let (mut width, mut height) = (0, 0);
        let id = unsafe { sys::krkr_scene_build(std::ptr::null_mut(), &mut width, &mut height) };
        let pixels = bevy_krkr::plugin::render::read_pixels(id).unwrap();
        let pixel = &pixels[((16 * width + 16) * 4) as usize..][..4];
        assert!(
            pixel[0].abs_diff(128) < 5 && pixel[1] < 5 && pixel[2].abs_diff(127) < 5,
            "{pixel:?}"
        );
        let shared = &pixels[((16 * width + 80) * 4) as usize..][..4];
        assert!(
            shared[0].abs_diff(128) < 5 && shared[1] < 5 && shared[2].abs_diff(127) < 5,
            "{shared:?}"
        );
        run("m.openMovie('video.mp4',false); m.startMovie(true);");
        for _ in 0..120 {
            tick();
        }
        assert_eq!(engine.eval("m.isPlayingMovie()").unwrap(), "1");
        run("m.stopMovie();");
        assert_eq!(engine.eval("stopped").unwrap(), "2");
        run(
            "m.onUpdateMovie=function(){ invalidate m; }; m.openMovie('video.mp4',false); m.startMovie(false);",
        );
        for _ in 0..30 {
            tick();
        }
        run("invalidate w;");
    }
    drop(engine);
    unsafe { sys::krkr_engine_shutdown() };
}
