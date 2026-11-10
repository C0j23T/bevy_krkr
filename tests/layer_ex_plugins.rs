mod common;
use bevy_krkr::{script::Tjs2Engine, sys};
use std::ffi::CString;

#[test]
fn perspective_raster_and_save() {
    common::initialize_renderer();
    let dir = std::env::temp_dir().join(format!("krkr-layer-plugins-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = CString::new(format!("{}/assets/game", env!("CARGO_MANIFEST_DIR"))).unwrap();
    assert_eq!(
        unsafe { sys::krkr_engine_init(path.as_ptr()) },
        sys::KRKR_OK
    );
    let engine = Tjs2Engine::new().unwrap();
    let run = |s: &str| engine.exec_script(s, "LayerPlugins").unwrap();
    let value = |s: &str| engine.eval(s).unwrap().parse::<u32>().unwrap();
    run(r#"
        Plugins.link("perspective.dll"); Plugins.link("layerExRaster.dll"); Plugins.link("layerExSave.dll");
        Plugins.link("perspective.dll"); Plugins.link("layerExRaster.dll"); Plugins.link("layerExSave.dll");
        global.w = new Window(); w.setInnerSize(16,16);
        global.s = new Layer(w,null); s.setImageSize(16,16); s.setSizeToImageSize();
        global.d = new Layer(w,s); d.assignImages(s);
        s.fillRect(0,0,16,16,0x7f204080); d.fillRect(0,0,16,16,0);
        d.perspectiveCopy(s,0,0,16,16,0,0,16,0,0,16,16,16);
    "#);
    assert_eq!(value("d.getMainPixel(4,4)"), 0x402010);
    assert_eq!(value("d.getMaskPixel(4,4)"), 127);
    run("d.perspectiveCopy(s,0,0,16,16,0,0,0,0,0,0,0,0);");
    run(
        "d.fillRect(0,0,16,16,0); d.setClip(4,4,3,3); d.perspectiveCopy(s,0,0,16,16,0,0,16,0,0,16,16,16);",
    );
    assert_eq!(value("d.getMaskPixel(3,4)"), 0);
    assert_eq!(value("d.getMaskPixel(4,4)"), 127);
    run(
        "d.setClip(0,0,16,16); d.fillRect(0,0,16,16,0); d.perspectiveCopy(s,0,0,16,16,2,1,14,3,0,14,15,15);",
    );
    assert_eq!(value("d.getMaskPixel(7,7)"), 127);
    assert_eq!(value("d.getMaskPixel(0,0)"), 0);
    run(
        "s.fillRect(0,0,16,16,0xff204080); d.assignImages(s); d.perspectiveCopy(d,0,0,16,16,0,0,16,0,0,16,16,16);",
    );
    assert_eq!(value("d.getMainPixel(4,4)"), 0x804020);
    for y in 0..16 {
        for x in 0..16 {
            run(&format!(
                "s.fillRect({x},{y},1,1,{});",
                (0x60u32 << 24) | ((x + 1) << 16) | (y << 8) | 7
            ));
        }
    }
    for (amplitude, lines, cycle, time) in [
        (3, 7, 100, 0),
        (-5, 9, 200, 79),
        (30, 11, 75, 23),
        (4, -13, -200, -17),
    ] {
        run(&format!(
            "d.setClip(0,0,16,16); d.fillRect(0,0,16,16,0xffabcdef); d.setClip(2,3,11,10); d.copyRaster(s,{amplitude},{lines},{cycle},{time});"
        ));
        let omega = 2.0 * std::f64::consts::PI / lines as f64;
        let mut rad = -omega * time as f64 / cycle as f64 * 8.0 + omega * 3.0;
        for y in 0..16 {
            let shift = if (3..13).contains(&y) {
                let shift = (rad.sin() * amplitude as f64).trunc() as i32;
                rad += omega;
                shift
            } else {
                0
            };
            for x in 0..16 {
                let from = x - shift;
                let copied =
                    (3..13).contains(&y) && (2..13).contains(&x) && (2..13).contains(&from);
                assert_eq!(
                    value(&format!("d.getMainPixel({x},{y})")),
                    if copied {
                        (7 << 16) | ((y as u32) << 8) | (from as u32 + 1)
                    } else {
                        0xefcdab
                    },
                    "raster {amplitude} {lines} {cycle} {time} at {x},{y}"
                );
                assert_eq!(
                    value(&format!("d.getMaskPixel({x},{y})")),
                    if copied { 96 } else { 255 }
                );
            }
        }
    }
    run("d.setClip(0,0,16,16); d.assignImages(s); d.copyRaster(d,0,7,100,0);");
    assert_eq!(value("d.getDiffPixel(s)"), 0);
    run("s.fillRect(0,0,16,16,0); s.fillRect(2,3,4,5,0x7f204080); global.crop=s.getCropRect();");
    assert_eq!(value("crop.x*1000+crop.y*100+crop.w*10+crop.h"), 2345);
    for ext in ["png", "tlg"] {
        let file = dir
            .join(format!("透明中文.{ext}"))
            .to_string_lossy()
            .replace('\\', "/");
        run(&format!(
            "s.saveLayerImage{}(\"{file}\", %[offs_x:2,offs_y:3]); d.loadImages(\"{file}\");",
            if ext == "png" { "Png" } else { "Tlg5" }
        ));
        assert_eq!(value("d.getDiffPixel(s)"), 0, "{ext} roundtrip");
        assert_eq!(value("d.getMaskPixel(2,3)"), 127);
        assert_eq!(value("d.getMainPixel(2,3)"), 0x804020);
    }
    run(
        "d.setImageSize(2,2); global.mismatch=0; try { s.getDiffRect(d); } catch(e) { global.mismatch=1; } d.setImageSize(16,16);",
    );
    assert_eq!(value("mismatch"), 1);
    run("global.bad=0; try { s.copyRaster(d,4,0,100,0); } catch(e) { global.bad=1; }");
    assert_eq!(value("bad"), 1);
    let province = dir.join("区域.png").to_string_lossy().into_owned();
    run(&format!(
        "s.setProvincePixel(0,0,5); s.saveProvinceImage(\"{province}\");"
    ));
    assert_eq!(
        &std::fs::read(&province).unwrap()[..8],
        b"\x89PNG\r\n\x1a\n"
    );
    let plain = dir.join("plain.tlg").to_string_lossy().into_owned();
    run(&format!(
        "s.saveLayerImageTlg5(\"{plain}\"); s.saveLayerImagePng(\"{plain}.png\",void);"
    ));
    for level in [-1, 0, 1, 6, 9] {
        let file = dir
            .join(format!("level-{level}.png"))
            .to_string_lossy()
            .into_owned();
        run(&format!(
            "s.saveLayerImagePng(\"{file}\", %[comp_lv:{level}]); d.loadImages(\"{file}\");"
        ));
        assert_eq!(value("d.getDiffPixel(s)"), 0, "PNG level {level}");
        run(&format!(
            "global.encoded=s.saveLayerImagePngOctet({level});"
        ));
        assert!(value("encoded.length") > 30);
    }
    run("d.fillRect(0,0,16,16,0);");
    run("global.changed=s.getDiffRect(d); global.fingerprint=s.getFingerPrintValue();");
    assert!(value("changed.w") > 0);
    run("global.samefinger=(s.getFingerPrintValue()==fingerprint);");
    assert_eq!(value("samefinger"), 1);
    run("global.o=s.saveLayerImagePngOctet();");
    assert!(value("o.length") > 30);
    run("d.assignImages(s); d.copyBlueToAlpha(s);");
    assert_eq!(value("d.getMaskPixel(2,3)"), 128);
    run("d.clearAlpha(128,0x123456);");
    assert_eq!(value("d.getMaskPixel(2,3)"), 0);
    assert_eq!(value("d.getMainPixel(2,3)"), 0x563412);
    let file = dir.join("异步.png").to_string_lossy().into_owned();
    run(&format!(
        r#"
        global.done=0; global.canceled=-1; global.progress=0;
        w.onSaveLayerImageProgress=function(h,p,l,f) {{ global.progress++; }};
        w.onSaveLayerImageDone=function(h,c,l,f) {{ global.done++; global.canceled=c; }};
        global.handle=w.startSaveLayerImage(s,"{file}",%[comp_lv:9]);
        s.fillRect(0,0,16,16,0xffffffff);
    "#
    ));
    for _ in 0..500 {
        assert_eq!(unsafe { sys::krkr_engine_tick() }, sys::KRKR_OK);
        if value("done") != 0 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert_eq!(value("done"), 1);
    assert_eq!(value("canceled"), 0);
    assert!(value("progress") > 0);
    run(&format!("d.loadImages(\"{file}\");"));
    assert_eq!(value("d.getMaskPixel(0,0)"), 0);
    assert_eq!(value("d.getMaskPixel(2,3)"), 127);
    run(&format!(
        "global.handle=w.startSaveLayerImage(s,\"{file}\",void); w.cancelSaveLayerImage(handle); global.done=0;"
    ));
    for _ in 0..500 {
        assert_eq!(unsafe { sys::krkr_engine_tick() }, sys::KRKR_OK);
        if value("done") != 0 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert_eq!(value("done"), 1);
    assert_eq!(value("canceled"), 1);
    run(&format!(
        "global.done=0; global.handle=w.startSaveLayerImage(s,\"{file}\",void); w.stopSaveLayerImage(handle);"
    ));
    for _ in 0..5 {
        assert_eq!(unsafe { sys::krkr_engine_tick() }, sys::KRKR_OK);
    }
    assert_eq!(value("done"), 0);
    let async_tlg = dir.join("异步.tlg").to_string_lossy().into_owned();
    run(&format!(
        "global.done=0; w.startSaveLayerImage(s,\"{async_tlg}\",%[caption:\"标签\"]);"
    ));
    for _ in 0..500 {
        assert_eq!(unsafe { sys::krkr_engine_tick() }, sys::KRKR_OK);
        if value("done") != 0 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert_eq!(value("done"), 1);
    run(&format!("d.loadImages(\"{async_tlg}\");"));
    assert_eq!(value("d.getDiffPixel(s)"), 0);

    run(
        "d.fillRect(0,0,16,16,0); d.fillRect(4,5,1,1,0x00123456); global.zero=d.getCropRectZero();",
    );
    assert_eq!(value("zero.x*100+zero.y"), 405);
    assert_eq!(engine.eval("typeof d.getCropRect()").unwrap(), "void");
    run("d.fillRect(4,5,1,1,0xff204080); d.oozeColor(1);");
    assert_eq!(value("d.getMainPixel(3,5)"), 0x804020);
    assert_eq!(value("d.getMaskPixel(3,5)"), 0);
    assert_eq!(value("d.getShrinkVectorOctet(4,4).length"), 64);
    run(&format!(
        "global.handle=w.startSaveLayerImage(s,\"{file}\",void); invalidate w;"
    ));
    drop(engine);
    unsafe { sys::krkr_engine_shutdown() };
    std::fs::remove_dir_all(dir).unwrap();
}
