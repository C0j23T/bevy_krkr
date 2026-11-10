mod common;
use bevy_krkr::{script::Tjs2Engine, sys};
use std::ffi::CString;

#[test]
fn layer_ex_image_gpu_pixels() {
    common::initialize_renderer();
    let path = CString::new(format!("{}/assets/game", env!("CARGO_MANIFEST_DIR"))).unwrap();
    assert_eq!(
        unsafe { sys::krkr_engine_init(path.as_ptr()) },
        sys::KRKR_OK
    );
    let engine = Tjs2Engine::new().unwrap();
    let run = |s: &str| engine.exec_script(s, "LayerExImage").unwrap();
    let value = |s: &str| engine.eval(s).unwrap().parse::<u32>().unwrap();
    run(r#"
        Plugins.link("layerExImage.dll"); Plugins.link("layerExImage.dll");
        global.w = new Window(); w.setInnerSize(16,16);
        global.l = new Layer(w,null); l.setImageSize(16,16); l.setSizeToImageSize();
        l.fillRect(0,0,16,16,0x7f204080);
        l.setClip(2,3,4,5); l.light(30,50);
    "#);
    assert_eq!(value("l.getMainPixel(2,3)"), 0x009e3e0e);
    assert_eq!(value("l.getMainPixel(1,3)"), 0x00804020);
    assert_eq!(value("l.getMaskPixel(2,3)"), 127);
    run("l.setClip(0,0,16,16); l.fillRect(0,0,16,16,0x7f808080); l.colorize(0,255,1.0);");
    assert_eq!(value("l.getMainPixel(0,0)"), 0x000101ff);
    run("l.fillRect(0,0,16,16,0x7fff0000); l.modulate(120,0,0);");
    assert_eq!(value("l.getMainPixel(0,0)"), 0x0000ff00);
    run("l.fillRect(0,0,16,16,0x7f808080); l.noise(80);");
    for y in 0..16 {
        for x in 0..16 {
            let rgb = value(&format!("l.getMainPixel({x},{y})"));
            for shift in [0, 8, 16] {
                assert!((88..=168).contains(&((rgb >> shift) & 255)));
            }
            assert_eq!(value(&format!("l.getMaskPixel({x},{y})")), 127);
        }
    }
    run("l.generateWhiteNoise();");
    let mut minimum = 255;
    let mut maximum = 0;
    for x in 0..16 {
        let rgb = value(&format!("l.getMainPixel({x},0)"));
        assert_eq!(rgb & 255, (rgb >> 8) & 255);
        assert_eq!(rgb & 255, (rgb >> 16) & 255);
        minimum = minimum.min(rgb & 255);
        maximum = maximum.max(rgb & 255);
    }
    assert!(maximum - minimum > 100);
    run("l.fillRect(0,0,16,16,0x7f204080); l.gaussianBlur(2.0);");
    assert_eq!(value("l.getMainPixel(0,0)"), 0x00804020);
    assert_eq!(value("l.getMaskPixel(0,0)"), 127);
    run(
        "l.setClip(0,0,16,16); l.fillRect(0,0,16,16,0xff204080); global.shared=new Layer(w,l); shared.assignImages(l); l.light(10,0);",
    );
    assert_eq!(value("shared.getMainPixel(0,0)"), 0x00804020);
    assert_eq!(value("l.getMainPixel(0,0)"), 0x008a4a2a);
    run("l.setSize(8,8); l.imageLeft=-3; l.imageTop=-2; l.setClip(4,5,2,2); l.light(10,0);");
    assert_eq!(value("l.getMainPixel(4,5)"), 0x00945434);
    assert_eq!(value("l.getMainPixel(3,5)"), 0x008a4a2a);
    run("l.setClip(0,0,0,0); l.gaussianBlur(2); l.setImageSize(16,16); l.setSizeToImageSize();");
    let reference = include_bytes!("fixtures/layer-ex-image.rgba");
    let mut offset = 0;
    for (width, height, operation) in [
        (16, 16, "light(30,50)"),
        (16, 16, "colorize(96,230,0.6)"),
        (16, 16, "modulate(-65,-45,25)"),
        (16, 16, "gaussianBlur(2.0)"),
        (3, 2, "gaussianBlur(2.0)"),
        (16, 16, "colorize(0,255,1)"),
    ] {
        run(&format!(
            "l.setImageSize({width},{height}); l.setSizeToImageSize(); l.setClip(0,0,{width},{height});"
        ));
        for y in 0..height {
            for x in 0..width {
                let r = (17 * x + 31 * y) % 256;
                let g = (53 * x + 7 * y) % 256;
                let b = (11 * x + 67 * y) % 256;
                let a = (19 * x + 23 * y) % 256;
                let color = (a << 24) | (r << 16) | (g << 8) | b;
                run(&format!("l.fillRect({x},{y},1,1,{color});"));
            }
        }
        run(&format!("l.{operation};"));
        for y in 0..height {
            for x in 0..width {
                let rgb = value(&format!("l.getMainPixel({x},{y})"));
                let alpha = value(&format!("l.getMaskPixel({x},{y})"));
                let actual = [rgb as u8, (rgb >> 8) as u8, (rgb >> 16) as u8, alpha as u8];
                let expected = &reference[offset..offset + 4];
                offset += 4;
                assert!(
                    actual
                        .iter()
                        .zip(expected)
                        .all(|(a, b)| a.abs_diff(*b) <= 1),
                    "{operation} {width}x{height} ({x},{y}): {actual:?} != {expected:?}"
                );
            }
        }
    }
    assert_eq!(offset, reference.len());
    drop(engine);
    unsafe { sys::krkr_engine_shutdown() };
}
