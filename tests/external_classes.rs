mod common;
use bevy_krkr::{script::Tjs2Engine, sys};
use std::ffi::CString;

#[test]
#[ignore = "requires ALPHA_MOVIE_PLUGIN and PSD_FILE_PLUGIN"]
fn external_classes_register_and_unload() {
    common::initialize_renderer();
    let project =
        std::env::temp_dir().join(format!("krkr-external-classes-{}", std::process::id()));
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(project.join("bad.amv"), b"bad movie").unwrap();
    let path = CString::new(project.to_str().unwrap()).unwrap();
    for _restart in 0..2 {
        for (variable, class) in [
            ("ALPHA_MOVIE_PLUGIN", "AlphaMovie"),
            ("PSD_FILE_PLUGIN", "PSD"),
        ] {
            let plugin = std::env::var(variable).expect(variable);
            assert_eq!(
                unsafe { sys::krkr_engine_init(path.as_ptr()) },
                sys::KRKR_OK
            );
            let engine = Tjs2Engine::new().unwrap();
            engine.exec_script(&format!("Plugins.link({plugin:?}); global.probe=new {class}(); invalidate probe; global.probe=void;"),class).unwrap();
            if class == "AlphaMovie" {
                let run = |script: &str| engine.exec_script(script, "AMV pixels").unwrap();
                run(
                    "global.w=new Window(); w.setInnerSize(16,16); w.visible=true; global.base=new Layer(w,null);base.setImageSize(16,16);base.setSizeToImageSize();base.type=ltAlpha;base.fillRect(0,0,16,16,0);global.d=new Layer(w,base);d.type=ltAlpha;d.visible=true;global.movie=new AlphaMovie();",
                );
                for mode in ["jpeg", "zlib"] {
                    run(&format!(
                        "movie.open('{}/tests/fixtures/external_{mode}.amv');",
                        env!("CARGO_MANIFEST_DIR")
                    ));
                    assert_eq!(engine.eval("movie.numOfFrame").unwrap(), "2");
                    for (n, expected) in
                        [[135, 73, 62, 64], [215, 153, 142, 192]].iter().enumerate()
                    {
                        assert_eq!(
                            engine.eval("movie.showNextImage(d)").unwrap(),
                            n.to_string()
                        );
                        let (mut width, mut height) = (0, 0);
                        let id = unsafe {
                            sys::krkr_scene_build(std::ptr::null_mut(), &mut width, &mut height)
                        };
                        assert_ne!(id, 0, "{}", bevy_krkr::script::last_error());
                        let rendered = bevy_krkr::plugin::render::read_pixels(id).unwrap();
                        for pixel in rendered.chunks_exact(4) {
                            for channel in 0..4 {
                                assert!(
                                    pixel[channel].abs_diff(expected[channel] as u8) <= 1,
                                    "{mode} frame {n} rendered {pixel:?}, expected {expected:?}"
                                );
                            }
                        }
                        assert_eq!(
                            engine.eval("d.getMaskPixel(8,8)").unwrap(),
                            expected[3].to_string()
                        );
                        let pixel = engine
                            .eval("d.getMainPixel(8,8)")
                            .unwrap()
                            .parse::<u32>()
                            .unwrap();
                        assert_eq!(
                            [pixel & 255, (pixel >> 8) & 255, (pixel >> 16) & 255],
                            [expected[0], expected[1], expected[2]],
                            "{mode} frame {n}"
                        );
                    }
                    assert_eq!(engine.eval("movie.showNextImage(d)").unwrap(), "1");
                    run("movie.loop=true;");
                    assert_eq!(engine.eval("movie.showNextImage(d)").unwrap(), "0");
                    run("movie.frame=1;");
                    assert_eq!(engine.eval("movie.showNextImage(d)").unwrap(), "1");
                    run("movie.loop=false;");
                }
                run("global.failed=false;try { movie.open('bad.amv'); }catch(e){failed=true;}");
                assert_eq!(engine.eval("failed").unwrap(), "1");
                run("invalidate movie;movie=void;invalidate w;w=void;");
            } else {
                let fixture = format!(
                    "{}/tests/fixtures/external_probe.psd",
                    env!("CARGO_MANIFEST_DIR")
                );
                engine.exec_script(&format!("global.psd=new PSD(); if(!psd.load({fixture:?})) throw 'PSD load failed';"),"PSD load").unwrap();
                assert_eq!(
                    engine
                        .eval("psd.width+','+psd.height+','+psd.layer_count")
                        .unwrap(),
                    "2,2,1"
                );
                engine.exec_script("global.w=new Window();w.setInnerSize(2,2);w.visible=true;global.base=new Layer(w,null);base.setImageSize(2,2);base.setSizeToImageSize();base.type=ltAlpha;base.fillRect(0,0,2,2,0);global.d=new Layer(w,base);psd.getLayerData(d,0);d.type=ltAlpha;d.visible=true;", "PSD layer").unwrap();
                let (mut width, mut height) = (0, 0);
                let id =
                    unsafe { sys::krkr_scene_build(std::ptr::null_mut(), &mut width, &mut height) };
                assert_ne!(id, 0, "{}", bevy_krkr::script::last_error());
                let pixels = bevy_krkr::plugin::render::read_pixels(id).unwrap();
                for (i, expected) in [
                    [210u8, 30, 70, 255],
                    [20, 190, 90, 128],
                    [40, 60, 220, 64],
                    [180, 120, 10, 255],
                ]
                .iter()
                .enumerate()
                {
                    for channel in 0..4 {
                        assert!(
                            pixels[i * 4 + channel].abs_diff(expected[channel]) <= 1,
                            "PSD pixel {i} channel {channel}: {:?}",
                            &pixels[i * 4..][..4]
                        );
                    }
                }
                assert_eq!(engine.eval("psd.getLayerInfo(0).name").unwrap(), "probe");
                assert_eq!(
                    engine
                        .eval(
                            "Storages.isExistentStorage('psd://external_probe.psd/root/probe.bmp')"
                        )
                        .unwrap(),
                    "1"
                );
                engine
                    .exec_script(
                        "d.loadImages('psd://external_probe.psd/root/probe.bmp');",
                        "PSD storage BMP",
                    )
                    .unwrap();
                let pixel = engine
                    .eval("d.getMainPixel(0,0)")
                    .unwrap()
                    .parse::<u32>()
                    .unwrap();
                assert_eq!(
                    [pixel & 255, (pixel >> 8) & 255, (pixel >> 16) & 255],
                    [210, 30, 70]
                );
                engine
                    .exec_script(
                        "PSD.clearStorageCache();invalidate psd;psd=void;invalidate w;w=void;",
                        "PSD cleanup",
                    )
                    .unwrap();
            }
            assert_eq!(
                engine.eval(&format!("Plugins.unlink({plugin:?})")).unwrap(),
                "1"
            );
            engine
                .exec_script(
                    &format!("Plugins.link({plugin:?});Plugins.link({plugin:?});"),
                    "relink",
                )
                .unwrap();
            assert_eq!(
                engine.eval(&format!("Plugins.unlink({plugin:?})")).unwrap(),
                "1"
            );
            drop(engine);
            unsafe { sys::krkr_engine_shutdown() };
        }
    }
    std::fs::remove_dir_all(project).unwrap();
}
