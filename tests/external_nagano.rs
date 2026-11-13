mod common;
use bevy_krkr::{script::Tjs2Engine, sys};
use std::ffi::CString;
#[test]
#[ignore = "requires EXT_NAGANO_PLUGIN built by tools/external-plugins/build.sh"]
fn all_nagano_transitions_match_reference() {
    let plugin = std::env::var("EXT_NAGANO_PLUGIN").expect("EXT_NAGANO_PLUGIN");
    common::initialize_renderer();
    let project = std::env::temp_dir().join(format!("krkr-external-nagano-{}", std::process::id()));
    std::fs::create_dir_all(&project).unwrap();
    std::fs::copy(
        format!("{}/assets/game/nagano_rule.bmp", env!("CARGO_MANIFEST_DIR")),
        project.join("nagano_rule.bmp"),
    )
    .unwrap();
    let path = CString::new(project.to_str().unwrap()).unwrap();
    for _restart in 0..2 {
        assert_eq!(
            unsafe { sys::krkr_engine_init(path.as_ptr()) },
            sys::KRKR_OK
        );
        let engine = Tjs2Engine::new().unwrap();
        let run = |s: &str| engine.exec_script(s, "Nagano reference").unwrap();
        run(&format!("Plugins.link({plugin:?});"));
        run(
            r#"global.w=new Window();w.setInnerSize(32,24);w.visible=true;global.base=new Layer(w,null);base.setImageSize(32,24);base.setSizeToImageSize();base.fillRect(0,0,32,24,0xff000000);global.d=new Layer(w,base);global.s=new Layer(w,base);global.rule=new Layer(w,base);rule.visible=false;rule.setImageSize(32,24);d.setImageSize(32,24);d.setSizeToImageSize();d.type=ltOpaque;s.setImageSize(32,24);s.setSizeToImageSize();s.type=ltOpaque;"#,
        );
        run(&format!("Plugins.link({plugin:?});"));
        let frame = || {
            let (mut w, mut h) = (0, 0);
            let id = unsafe { sys::krkr_scene_build(std::ptr::null_mut(), &mut w, &mut h) };
            assert_ne!(id, 0, "{}", bevy_krkr::script::last_error());
            bevy_krkr::plugin::render::read_pixels(id).unwrap()
        };
        let mut refs = include_bytes!("fixtures/nagano.rgb").chunks_exact(3);
        for (method, opts) in [
            ("scanline", ""),
            ("zoomfade", "zoom1:150,zoom2:200"),
            ("rgbfade", "delayR:0,delayG:80,delayB:160,delayA:40"),
            ("spin", "type1:4,type2:7"),
            ("flutter", "back:0x802468ac,alpha:180,slip:8"),
            ("book", "dir:1"),
            ("imagewipe", "rule:rule,dir:0"),
            ("honeyturn", "size:8,twist:3,order:2,dir:6"),
            (
                "morphing",
                "before:[0,0,32,0,32,24,0,0,32,24,0,24],after:[0,0,32,2,30,24,2,0,32,22,0,24]",
            ),
            ("multiripple", "count:1,rwidth:8,wavecount:2,maxdrift:4"),
            ("3duniversal", "rule:\"nagano_rule.bmp\",speed1:1"),
            ("blurfade", "blur1x:4,blur1y:3,blur2x:2,blur2y:4"),
        ]
        .iter()
        {
            run(
                "d.stopTransition();global.completed=0;d.onTransitionCompleted=function(dest,src){completed++;};d.visible=true;s.visible=false;",
            );
            for y in 0..24 {
                for x in 0..32 {
                    let a = 0xff000000u32
                        | (((x * 5 + y * 7) % 256) << 16)
                        | (((x * 17 + y * 2) % 256) << 8)
                        | (x * 3 + y * 23) % 256;
                    let b = 0xff000000u32
                        | (((x * 11 + y * 3) % 256) << 16)
                        | (((x * 2 + y * 13) % 256) << 8)
                        | (x * 19 + y * 5) % 256;
                    let r = 0xff000000u32
                        | ((x * 8 % 256) << 16)
                        | ((y * 10 % 256) << 8)
                        | (x * 7 % 256);
                    run(&format!(
                        "d.fillRect({x},{y},1,1,{a});s.fillRect({x},{y},1,1,{b});rule.fillRect({x},{y},1,1,{r});"
                    ));
                }
            }
            run(&format!(
                "global.tick=0;d.beginTransition('{method}',false,s,%[time:100,selfupdate:true,callback:function(){{return tick;}},{opts}]);"
            ));
            if opts.contains("rule:\"") {
                // Rule images decode on the engine's worker thread; wait for the
                // pixels so the transition clock starts at tick 0.
                std::thread::sleep(std::time::Duration::from_millis(300));
            }
            frame();
            for t in [1, 25, 50, 75, 99] {
                run(&format!("tick={t};"));
                let pixels = frame();
                for y in 0..24 {
                    for x in 0..32 {
                        let expected = refs.next().unwrap();
                        let actual = &pixels[(y * 32 + x) * 4..][..3];
                        assert_eq!(actual, expected, "{method} at {t}% pixel ({x},{y})");
                    }
                }
            }
            run("tick=100;");
            frame();
            assert_eq!(
                engine.eval("completed").unwrap(),
                "1",
                "{method} completion"
            );
        }

        assert!(refs.next().is_none());
        run("d.stopTransition();");
        assert_eq!(
            engine.eval(&format!("Plugins.unlink({plugin:?})")).unwrap(),
            "1"
        );
        drop(engine);
        unsafe { sys::krkr_engine_shutdown() };
    }
    std::fs::remove_dir_all(project).unwrap();
}
