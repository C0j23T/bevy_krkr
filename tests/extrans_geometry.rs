mod common;

use bevy_krkr::{
    script::{Tjs2Engine, last_error},
    sys,
};
use std::ffi::CString;

fn frame() -> Vec<u8> {
    let (mut width, mut height) = (0, 0);
    let id = unsafe { sys::krkr_scene_build(std::ptr::null_mut(), &mut width, &mut height) };
    assert_ne!(id, 0, "{}", last_error());
    assert_eq!((width, height), (130, 98));
    bevy_krkr::plugin::render::read_pixels(id).unwrap()
}

#[test]
fn turn_and_rotations_match_upstream_in_both_directions() {
    common::initialize_renderer();
    let path = CString::new(format!("{}/assets/game", env!("CARGO_MANIFEST_DIR"))).unwrap();
    assert_eq!(
        unsafe { sys::krkr_engine_init(path.as_ptr()) },
        sys::KRKR_OK
    );
    let engine = Tjs2Engine::new().unwrap();
    let run = |script: &str| engine.exec_script(script, "extrans geometry").unwrap();
    run(r#"
        Plugins.link("extrans.dll");
        global.w=new Window(); w.setInnerSize(130,98); w.visible=true;
        global.base=new Layer(w,null); base.setImageSize(130,98); base.setSizeToImageSize();
        global.d=new Layer(w,base); d.setImageSize(130,98); d.setSizeToImageSize(); d.type=ltOpaque;
        global.s=new Layer(w,base); s.setImageSize(130,98); s.setSizeToImageSize(); s.type=ltOpaque;
    "#);
    let mut reference = include_bytes!("fixtures/extrans_geometry.rgb").chunks_exact(3);
    for (method, options) in [
        ("turn", "bgcolor:0xff172b49"),
        (
            "rotatezoom",
            "factor:0.35,accel:-1.5,twist:1.3,twistaccel:0.7,centerx:41,centery:29",
        ),
        (
            "rotatevanish",
            "accel:1.7,twist:-1.1,twistaccel:1.4,centerx:41,centery:29",
        ),
        ("rotateswap", "bgcolor:0xff172b49,twist:1.3"),
        (
            "rotatezoom",
            "factor:1.6,accel:0.5,twist:1.5,twistaccel:1.4,centerx:65,centery:49",
        ),
        (
            "rotatezoom",
            "factor:0,accel:1,twist:2,twistaccel:1,centerx:65,centery:49",
        ),
    ] {
        for direction in 0..2 {
            run(&format!(
                r#"
                d.visible=true; s.visible=false;
                d.loadImages("{}/tests/fixtures/extrans_{}.bmp"); s.loadImages("{}/tests/fixtures/extrans_{}.bmp"); global.tick=0;
                global.frameCompletions=0;
                d.onTransitionCompleted=function(dest,src) {{ frameCompletions++; }};
                d.beginTransition("{method}",false,s,%[time:100,selfupdate:true,callback:function(){{return tick;}},{options}]);
            "#,
                env!("CARGO_MANIFEST_DIR"),
                if direction == 0 { "a" } else { "b" },
                env!("CARGO_MANIFEST_DIR"),
                if direction == 0 { "b" } else { "a" }
            ));
            frame();
            for time in [1, 25, 50, 75, 95, 99] {
                run(&format!("global.tick={time};"));
                let pixels = frame();
                assert_eq!(
                    engine.eval("frameCompletions").unwrap(),
                    "0",
                    "{method} completed at {time}%"
                );
                for y in (1..98).step_by(3) {
                    for x in (1..130).step_by(3) {
                        let expected = reference.next().unwrap();
                        let actual = &pixels[(y * 130 + x) * 4..][..3];
                        assert_eq!(
                            actual, expected,
                            "{method} direction {direction} time {time} pixel {x},{y}"
                        );
                    }
                }
            }
            run("global.tick=100;");
            frame();
            assert_eq!(
                engine.eval("frameCompletions").unwrap(),
                "1",
                "{method} completion at 100%"
            );
            run("d.stopTransition();");
        }
    }
    assert!(reference.next().is_none());
    run(r#"
        global.sequence=["rotatezoom","rotatevanish","rotateswap","turn"];
        global.sequenceIndex=0; global.pending=true; global.completed=0; global.durations=[];
        d.onTransitionCompleted=function(dest,src) {
            durations.push(System.getTickCount()-started); completed++; pending=true;
        };
        global.timerOwner=%[]; global.timer=new Timer(timerOwner);
        timer.interval=5;
        timer.onTimer=function() {
            if(!pending || sequenceIndex>=8) return;
            pending=false; d.visible=true; s.visible=false;
            var method=sequence[sequenceIndex%4];
            global.started=System.getTickCount();
            d.beginTransition(method,false,s,%[time:60,twist:1.3,factor:0.35,bgcolor:0xff172b49]);
            sequenceIndex++;
        };
        timer.enabled=true;
    "#);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while engine.eval("completed").unwrap() != "8" && std::time::Instant::now() < deadline {
        assert_eq!(
            unsafe { sys::krkr_engine_tick() },
            sys::KRKR_OK,
            "{}",
            last_error()
        );
        frame();
        std::thread::sleep(std::time::Duration::from_millis(8));
    }
    assert_eq!(engine.eval("sequenceIndex").unwrap(), "8");
    assert_eq!(engine.eval("completed").unwrap(), "8");
    for duration in engine.eval("durations.join(',')").unwrap().split(',') {
        assert!(
            duration.parse::<u64>().unwrap() >= 60,
            "early completion: {duration} ms"
        );
    }
    run("timer.enabled=false; invalidate timer;");
    run("w.visible=false;");
    drop(engine);
    unsafe { sys::krkr_engine_shutdown() };
}
