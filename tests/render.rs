mod common;

use bevy_krkr::{
    script::{last_error, Tjs2Engine},
    sys,
};
use std::ffi::CString;

fn frame() -> (usize, usize, Vec<u8>) {
    let (mut w, mut h) = (0, 0);
    let id = unsafe { sys::krkr_scene_build(std::ptr::null_mut(), &mut w, &mut h) };
    assert_ne!(id, 0, "{}", last_error());
    (
        w as usize,
        h as usize,
        bevy_krkr::plugin::render::read_pixels(id).unwrap(),
    )
}
fn pixel(x: usize, y: usize) -> [u8; 4] {
    let (w, h, bytes) = frame();
    assert!(x < w && y < h);
    bytes[(y * w + x) * 4..][..4].try_into().unwrap()
}
fn script(engine: &Tjs2Engine, source: &str) {
    engine.exec_script(source, "render test").unwrap();
}
fn near(actual: [u8; 4], expected: [u8; 4]) {
    assert!(
        actual
            .iter()
            .zip(expected)
            .all(|(&a, b)| a.abs_diff(b) <= 2),
        "{actual:?} != {expected:?}"
    );
}

#[test]
fn gpu_layer_operations() {
    common::initialize_renderer();
    let path = CString::new(format!("{}/assets/game", env!("CARGO_MANIFEST_DIR"))).unwrap();
    assert_eq!(
        unsafe { sys::krkr_engine_init(path.as_ptr()) },
        sys::KRKR_OK
    );
    let engine = Tjs2Engine::new().unwrap();
    script(
        &engine,
        r#"
        global.w = new Window(); w.setInnerSize(64,48); w.visible = true;
        global.base = new Layer(w,null); base.setImageSize(64,48); base.setSizeToImageSize();
        base.fillRect(0,0,64,48,0xff0000ff);
        function layer(parent,width,height,color) {
            var l = new Layer(w,parent); l.type=ltAlpha;
            l.setImageSize(width,height); l.setSizeToImageSize();
            l.fillRect(0,0,width,height,color); l.visible=true; return l;
        }
        global.group = layer(base,24,24,0xffff0000); group.setPos(4,4); group.opacity=128;
        global.child = layer(group,12,12,0xff00ff00); child.setPos(6,6);
    "#,
    );
    near(pixel(5, 5), [128, 0, 127, 255]);
    near(pixel(12, 12), [0, 128, 127, 255]);
    assert_eq!(pixel(30, 30), [0, 0, 255, 255]);
    script(&engine, "child.setPos(20,20);");
    near(pixel(26, 26), [0, 128, 127, 255]);
    assert_eq!(pixel(29, 29), [0, 0, 255, 255]);
    script(
        &engine,
        r#"
        group.visible=false;
        global.src=layer(base,4,4,0xffff0000); src.visible=false;
        src.fillRect(2,0,2,4,0xff00ff00);
        global.dst=layer(base,16,16,0); dst.setPos(0,0);
        dst.stretchCopy(0,0,8,8,src,0,0,4,4,stNearest);
    "#,
    );
    assert_eq!(pixel(1, 1), [255, 0, 0, 255]);
    assert_eq!(pixel(6, 1), [0, 255, 0, 255]);
    script(
        &engine,
        r#"
        dst.copyRect(2,0,dst,0,0,6,8);
        dst.affineCopy(src,0,0,4,4,true,1,0,0,1,8,8,stNearest);
    "#,
    );
    assert_eq!(pixel(3, 1), [255, 0, 0, 255]);
    assert_eq!(pixel(7, 1), [0, 255, 0, 255]);
    assert_eq!(pixel(9, 9), [255, 0, 0, 255]);
    assert_eq!(pixel(11, 9), [0, 255, 0, 255]);
    script(
        &engine,
        "dst.fillRect(0,0,16,16,0xff000000); dst.fillRect(8,8,1,1,0xffffffff); dst.doBoxBlur(1,1);",
    );
    near(pixel(8, 8), [28, 28, 28, 255]);
    assert_eq!(pixel(6, 6), [0, 0, 0, 255]);
    script(
        &engine,
        "dst.fillRect(0,0,16,16,0xff404040); dst.adjustGamma(2,0,255,2,0,255,2,0,255);",
    );
    near(pixel(8, 8), [128, 128, 128, 255]);
    for (mode, expected) in [
        ("omAdditive", 192),
        ("omSubtractive", 0),
        ("omMultiplicative", 32),
        ("omDodge", 171),
        ("omDarken", 64),
        ("omLighten", 128),
        ("omScreen", 160),
        ("omPsOverlay", 65),
        ("omPsHardLight", 64),
        ("omPsSoftLight", 96),
        ("omPsDifference", 64),
        ("omPsExclusion", 128),
        ("omPsColorBurn", 0),
    ] {
        script(&engine,&format!("dst.fillRect(0,0,16,16,0xff808080); src.fillRect(0,0,4,4,0xff404040); dst.operateRect(0,0,src,0,0,4,4,{mode});"));
        near(pixel(1, 1), [expected, expected, expected, 255]);
    }
    for filter in [
        "stCubic",
        "stLanczos2",
        "stLanczos3",
        "stSpline16",
        "stSpline36",
        "stAreaAvg",
        "stGaussian",
        "stBlackmanSinc",
    ] {
        script(&engine,&format!("src.fillRect(0,0,4,4,0xff406080); dst.stretchCopy(0,0,16,16,src,0,0,4,4,{filter});"));
        near(pixel(8, 8), [64, 96, 128, 255]);
    }
    // Kirikiri's non-Photoshop dodge scales the source BEFORE division.
    // A white effect layer at opacity 240 must not saturate every nonzero
    // background channel, even when that layer's own alpha is zero.
    script(
        &engine,
        "src.fillRect(0,0,4,4,0x00ffffff); src.bringToFront();",
    );
    for opacity in [0, 128, 240, 255] {
        for value in [0u32, 1, 17, 64, 128, 255] {
            let color = 0xff000000 | value * 0x010101;
            // TVPColorDodgeBlend_HDA[_o]_c and TVPRecipTable256.
            let source = if opacity == 255 {
                255
            } else {
                255 * opacity / 256
            };
            let reciprocal = 65536 / (255 - source).max(1);
            let expected = if opacity == 0 {
                value
            } else {
                (value * reciprocal / 256).min(255)
            } as u8;
            script(&engine, &format!(
                "dst.fillRect(0,0,16,16,{color}); dst.operateRect(0,0,src,0,0,4,4,omDodge,{opacity});"
            ));
            assert_eq!(pixel(1, 1), [expected, expected, expected, 255]);
            script(&engine, &format!(
                "dst.fillRect(0,0,16,16,{color}); src.type=ltDodge; src.opacity={opacity}; src.visible=true;"
            ));
            assert_eq!(pixel(1, 1), [expected, expected, expected, 255]);
            script(&engine, "src.visible=false; src.type=ltAlpha;");
        }
    }
    script(
        &engine,
        r#"
        dst.fillRect(0,0,16,16,0xff000000);
        dst.setMainPixel(1,1,0xff0000); dst.setMaskPixel(1,1,128);
    "#,
    );
    near(pixel(1, 1), [128, 0, 127, 255]);
    script(&engine, "dst.flipLR(); dst.flipUD();");
    near(pixel(14, 14), [128, 0, 127, 255]);
    script(
        &engine,
        r#"
        global.patch=layer(base,5,5,0); patch.visible=false;
        patch.fillRect(1,1,3,3,0xffff0000);
        patch.fillRect(2,0,1,1,0xff000000); patch.fillRect(0,2,1,1,0xff000000);
        patch.fillRect(2,4,1,1,0xff000000); patch.fillRect(4,2,1,1,0xff000000);
        patch.fillRect(2,2,1,1,0xff00ff00);
        dst.copy9Patch(patch);
    "#,
    );
    near(pixel(0, 0), [255, 0, 0, 255]);
    near(pixel(8, 8), [0, 255, 0, 255]);
    near(pixel(15, 15), [255, 0, 0, 255]);
    script(
        &engine,
        "dst.fillRect(0,0,16,16,0); dst.font.height=12; dst.drawText(0,0,\"A\",0xffffff);",
    );
    let (w, _, bytes) = frame();
    assert!((0..16).any(|y| (0..16).any(|x| bytes[(y * w + x) * 4] > 0)));
    script(
        &engine,
        r#"
        dst.fillRect(0,0,16,16,0xffff0000);
        global.next=layer(base,16,16,0xff00ff00); next.visible=false;
        global.tick=0;
        dst.beginTransition("crossfade",true,next,%["time"=>100,"selfupdate"=>true,"callback"=>function() { return global.tick; }]);
    "#,
    );
    near(pixel(8, 8), [255, 0, 0, 255]);
    script(&engine, "global.tick=50;");
    near(pixel(8, 8), [128, 127, 0, 255]);
    script(&engine, "global.tick=100;");
    near(pixel(8, 8), [0, 255, 0, 255]);
    script(&engine, "w.visible=false;");
    let (mut w, mut h) = (0, 0);
    assert_eq!(
        unsafe { sys::krkr_scene_build(std::ptr::null_mut(), &mut w, &mut h) },
        0
    );
    drop(engine);
    unsafe { sys::krkr_engine_shutdown() };
}
