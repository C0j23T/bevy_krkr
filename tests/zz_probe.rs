mod common;
use bevy_krkr::{script::{Tjs2Engine,last_error}, sys};
use std::ffi::CString;

fn frame() -> (Vec<u8>, u32) {
    let (mut width, mut height) = (0, 0);
    let id = unsafe { sys::krkr_scene_build(std::ptr::null_mut(), &mut width, &mut height) };
    assert_ne!(id, 0, "{}", last_error());
    (bevy_krkr::plugin::render::read_pixels(id).unwrap(), width as u32)
}

#[test]
fn probe() {
    common::initialize_renderer();
    let root = CString::new(format!("{}/test_games/extrans_showcase", env!("CARGO_MANIFEST_DIR"))).unwrap();
    assert_eq!(unsafe { sys::krkr_engine_init(root.as_ptr()) }, sys::KRKR_OK);
    let engine = Tjs2Engine::new().unwrap();
    let run = |s: &str| engine.exec_script(s, "probe").unwrap();
    run(include_str!("../test_games/extrans_showcase/startup.tjs"));
    run("showcaseTimer.enabled=false;");
    for effect in ["wave", "scanline"] {
        run(&format!(r#"
            try {{ showcaseTo.stopTransition(); }} catch(e) {{}}
            showcaseFrom.visible=false; showcaseTo.visible=true;
            global.tick=0;
            var i;
            for(i=0;i<2560;i++) {{
                showcaseFrom.fillRect(i,0,1,1600,0xff000000|((i%32)*8<<16));
                showcaseTo.fillRect(i,0,1,1600,0xff000000|((i%32)*8<<8));
            }}
            global.options=showcaseOptions["{effect}"];
            options.time=5000; options.selfupdate=false;
            options.callback=function(){{return tick;}};
            showcaseTo.beginTransition("{effect}",false,showcaseFrom,options);
        "#));
        run("tick=2500;");
        unsafe { sys::krkr_engine_tick() };
        let (pixels, w) = frame();
        let mut report = Vec::new();
        for y in [0u32, 100, 400, 800, 1200, 1599] {
            let row = &pixels[(y * w * 4) as usize..];
            let g = row[1] as i32;
            let shift = ((g - 0) / 8) as i32;
            report.push(format!("y{y}=>{shift}"));
        }
        eprintln!("{effect}: {}", report.join(" "));
    }
}
