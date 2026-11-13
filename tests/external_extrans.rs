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

fn reference_pixel(x: i32, y: i32, second: bool) -> [i32; 3] {
    if second {
        [
            (x * 11 + y * 3) % 256,
            (x * 2 + y * 13) % 256,
            (x * 19 + y * 5) % 256,
        ]
    } else {
        [
            (x * 5 + y * 7) % 256,
            (x * 17 + y * 2) % 256,
            (x * 3 + y * 23) % 256,
        ]
    }
}

#[test]
#[ignore = "requires EXTRANS_PLUGIN built by tools/extrans/build.sh"]
fn external_plugin_pixels_and_lifecycle() {
    let plugin = std::env::var("EXTRANS_PLUGIN").expect("EXTRANS_PLUGIN=/absolute/path/extrans.so");
    let plugin = std::fs::canonicalize(plugin).unwrap();
    let load = format!("Plugins.link({:?});", plugin.to_str().unwrap());
    common::initialize_renderer();
    let project =
        std::env::temp_dir().join(format!("krkr-external-extrans-{}", std::process::id()));
    std::fs::create_dir_all(&project).unwrap();
    let path = CString::new(project.to_str().unwrap()).unwrap();
    assert_eq!(
        unsafe { sys::krkr_engine_init(path.as_ptr()) },
        sys::KRKR_OK
    );
    let engine = Tjs2Engine::new().unwrap();
    let run = |script: &str| engine.exec_script(script, "extrans geometry").unwrap();
    run(&load);
    run(&load);
    run(r#"
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
    for method in ["wave", "mosaic", "ripple"] {
        run(&format!(
            r#"
            d.visible=true; s.visible=false;
            d.loadImages("{}/tests/fixtures/extrans_a.bmp");
            s.loadImages("{}/tests/fixtures/extrans_b.bmp");
            global.tick=0; global.frameCompletions=0;
            d.onTransitionCompleted=function(dest,src) {{ frameCompletions++; }};
            d.beginTransition("{method}",false,s,%[time:100,selfupdate:true,
                callback:function(){{return tick;}},maxh:6,maxomega:0.4,maxsize:10,bgcolor1:0xff204060,bgcolor2:0xff608020,
                rwidth:16,roundness:1,speed:6,maxdrift:6]);
        "#,
            env!("CARGO_MANIFEST_DIR"),
            env!("CARGO_MANIFEST_DIR")
        ));
        let initial = frame();
        assert_eq!(
            engine
                .eval(&format!("Plugins.unlink({:?})", plugin.to_str().unwrap()))
                .unwrap(),
            "0"
        );
        for time in [25, 50, 75] {
            run(&format!("global.tick={time};"));
            let middle = frame();
            assert_ne!(
                initial, middle,
                "{method} must produce an intermediate image"
            );
            assert_eq!(engine.eval("frameCompletions").unwrap(), "0");
            if method == "ripple" {
                continue;
            }
            let opacity = time * 255 / 100;
            for y in 0..98i32 {
                for x in 0..130i32 {
                    let (mut qx, mut qy) = (x, y);
                    if method == "wave" {
                        let envelope =
                            (std::f64::consts::FRAC_PI_2 * time.min(100 - time) as f64 / 50.).sin();
                        let amplitude = (envelope * 6.) as i32;
                        qx -= ((0.4 * envelope * (y - 49) as f64).sin() * amplitude as f64) as i32;
                    } else {
                        let size = 8 * time.min(100 - time) / 50 + 2;
                        let sample = |v: i32, extent: i32| {
                            let o = (extent - size) / 2 - (extent / 2 / size) * size;
                            let offset = if o > 0 { o - size } else { o };
                            (((v - offset) as f64 / size as f64).floor() as i32 * size
                                + offset
                                + size / 2)
                                .clamp(0, extent - 1)
                        };
                        qx = sample(x, 130);
                        qy = sample(y, 98);
                    }
                    let (a, b) = if (0..130).contains(&qx) {
                        (
                            reference_pixel(qx, qy, false),
                            reference_pixel(qx, qy, true),
                        )
                    } else {
                        ([32, 64, 96], [96, 128, 32])
                    };
                    for channel in 0..3 {
                        let expected = a[channel] + ((b[channel] - a[channel]) * opacity >> 8);
                        let actual = middle[((y * 130 + x) * 4) as usize + channel] as i32;
                        assert!(
                            (actual - expected).abs() <= 1,
                            "{method} {time}% pixel {x},{y} channel {channel}: {actual} != {expected}"
                        );
                    }
                }
            }
        }
        run("global.tick=100;");
        let final_image = frame();
        assert_ne!(initial, final_image);
        assert_eq!(engine.eval("frameCompletions").unwrap(), "1");
        run("d.stopTransition();");
    }
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
    assert_eq!(
        engine
            .eval(&format!("Plugins.unlink({:?})", plugin.to_str().unwrap()))
            .unwrap(),
        "1"
    );
    run(&load);
    run(
        "w.visible=true; d.visible=true; s.visible=false; global.tick=0; d.beginTransition('mosaic',false,s,%[time:100,selfupdate:true,callback:function(){return tick;}]);",
    );
    frame();
    // Shut down with an active external handler to verify deferred dlclose.

    drop(engine);
    unsafe { sys::krkr_engine_shutdown() };
    assert_eq!(
        unsafe { sys::krkr_engine_init(path.as_ptr()) },
        sys::KRKR_OK
    );
    let engine = Tjs2Engine::new().unwrap();
    engine.exec_script(&load, "reload after shutdown").unwrap();
    assert_eq!(
        engine
            .eval(&format!("Plugins.unlink({:?})", plugin.to_str().unwrap()))
            .unwrap(),
        "1"
    );
    drop(engine);
    unsafe { sys::krkr_engine_shutdown() };
    std::fs::remove_dir_all(project).unwrap();
}
