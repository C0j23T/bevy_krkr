mod common;
use bevy_krkr::{
    script::{Tjs2Engine, last_error},
    sys,
};
use std::ffi::CString;
fn frame() -> Vec<u8> {
    let (mut w, mut h) = (0, 0);
    let id = unsafe { sys::krkr_scene_build(std::ptr::null_mut(), &mut w, &mut h) };
    assert_ne!(id, 0, "{}", last_error());
    assert_eq!((w, h), (32, 24));
    bevy_krkr::plugin::render::read_pixels(id).unwrap()
}
fn rgba(x: usize, y: usize, second: bool) -> [u8; 4] {
    if second {
        [(x * 5) as u8, (y * 9) as u8, 180, 255]
    } else {
        [200, (x * 7) as u8, (y * 10) as u8, 255]
    }
}
#[test]
fn text_and_gpu_transitions_restart() {
    common::initialize_renderer();
    let path = CString::new(format!("{}/assets/game", env!("CARGO_MANIFEST_DIR"))).unwrap();
    for restart in 0..2 {
        assert_eq!(
            unsafe { sys::krkr_engine_init(path.as_ptr()) },
            sys::KRKR_OK
        );
        let engine = Tjs2Engine::new().unwrap();
        let run = |s: &str| engine.exec_script(s, "extra plugins").unwrap();
        let value = |s: &str| engine.eval(s).unwrap().parse::<i64>().unwrap();
        run(r#"
            Plugins.link("extrans.dll"); Plugins.link("extNagano.dll");
            Plugins.link("layerExText.dll"); Plugins.link("layerExDraw.dll");
            Plugins.link("extrans.dll"); Plugins.link("extNagano.dll");
            global.w=new Window(); w.setInnerSize(32,24); w.visible=true;
            global.base=new Layer(w,null); base.setImageSize(32,24); base.setSizeToImageSize(); base.fillRect(0,0,32,24,0);
            global.d=new Layer(w,base); d.setImageSize(32,24); d.setSizeToImageSize(); d.visible=true;
            global.s=new Layer(w,base); s.setImageSize(32,24); s.setSizeToImageSize(); s.visible=false;
            global.font=new GdiPlus.Font("DejaVu Sans",16,0);
            global.app=new GdiPlus.Appearance(); app.addBrush(0xffffffff);
            d.type=ltAlpha; d.fillRect(0,0,32,24,0); d.font.face="DejaVu Sans"; d.font.height=-16;
            d.drawText(2,1,"Ag",0xffffff);
            s.type=ltAlpha; s.fillRect(0,0,32,24,0); s.drawString(font,app,2,1,"Ag");
        "#);
        let mut covered = 0;
        for y in 0..24 {
            for x in 0..32 {
                let a = value(&format!("d.getMaskPixel({x},{y})"));
                let b = value(&format!("s.getMaskPixel({x},{y})"));
                assert!((a - b).abs() <= 1, "glyph at {x},{y}: {a} {b}");
                covered += a;
            }
        }
        assert!(covered > 1000);
        assert!(value("int(d.measureString(font,'Ag').width)") > 10);
        assert!(value("int(font.ascent)") > 0);
        assert!(value("int(font.lineSpacing)") >= 16);
        assert!(value("int(font.ascentLeading)") >= 0);
        assert!(value("int(font.descentLeading)") >= 0);
        assert_eq!(value("typeof GdiPlus.getFontList(false) == 'Object' ? 1 : 0"), 1);
        assert_eq!(value("GdiPlus.getFontList(true).length"), 0);
        assert!(value("int(d.measureStringInternal(font,'Ag').height)") > 5);
        run(r#"
            app.clear(); app.addPen(0xff0000ff,4); app.addBrush(0x80ff0000,2,1);
            s.fillRect(0,0,32,24,0); s.setClip(5,4,20,16); s.drawPathString(font,app,2,1,"Ag");
        "#);
        assert_eq!(value("s.getMaskPixel(3,5)"), 0);
        assert!(value("s.getMaskPixel(10,10)") > 0);
        run(r#"
            app.clear(); app.addBrush(%[type:4,color1:0xffff0000,color2:0xff0000ff,point1:[0,0],point2:[32,0]],0,0);
            s.setClip(0,0,32,24); s.fillRect(0,0,32,24,0); s.drawString(font,app,2,1,"MMMM");
        "#);
        let mut red = 0;
        let mut blue = 0;
        for y in 0..24 {
            for x in 0..32 {
                if value(&format!("s.getMaskPixel({x},{y})")) > 128 {
                    let c = value(&format!("s.getMainPixel({x},{y})"));
                    if x < 12 {
                        red += c & 255;
                    } else if x > 20 {
                        blue += (c >> 16) & 255;
                    }
                }
            }
        }
        assert!(red > 1000 && blue > 1000, "gradient {red} {blue}");
        run(r#"
            app.clear(); app.addBrush(%[type:4,color1:0xffff0000,color2:0xff0000ff,rect:[0,0,32,24],angle:45,wrapMode:4],0,0);
            s.fillRect(0,0,32,24,0); s.drawString(font,app,2,1,"MMMM");
        "#);
        assert!(value("s.getMaskPixel(10,10)") > 0);
        run("d.type=ltOpaque; s.type=ltOpaque; d.setClip(0,0,32,24);");
        for method in ["wave", "mosaic", "blurfade"] {
            run("d.visible=true; s.visible=false;");
            for y in 0..24 {
                for x in 0..32 {
                    let a = rgba(x, y, false);
                    let b = rgba(x, y, true);
                    let packed = |c: [u8; 4]| {
                        0xff000000u32 | ((c[0] as u32) << 16) | ((c[1] as u32) << 8) | c[2] as u32
                    };
                    run(&format!(
                        "d.fillRect({x},{y},1,1,{});s.fillRect({x},{y},1,1,{});",
                        packed(a),
                        packed(b)
                    ));
                }
            }
            run(&format!(
                r#"global.tick=0; d.beginTransition("{method}",false,s,%[time:100,selfupdate:true,callback:function() {{ return global.tick; }},maxh:6,maxomega:0.4,bgcolor1:0xff204060,bgcolor2:0xff608020,maxsize:10,blur1:4,blur2:4]);"#
            ));
            for time in [0, 25, 50, 75, 100] {
                run(&format!("global.tick={time};"));
                let pixels = frame();
                for y in 0..24usize {
                    for x in 0..32usize {
                        let mut qx = x as i32;
                        let mut qy = y as i32;
                        let f = if method == "blurfade" {
                            (time as f64 * 255. / 100.).round() / 255.
                        } else {
                            (time * 255 / 100) as f64 / 255.
                        };
                        let mut expected = [0.; 4];
                        if time == 0 || time == 100 {
                            expected = rgba(x, y, time == 100).map(f64::from);
                        } else if method == "wave" {
                            let envelope =
                                (std::f64::consts::FRAC_PI_2 * (time.min(100 - time) as f64) / 50.)
                                    .sin();
                            let amplitude = (envelope * 6.) as i32;
                            qx -= ((0.4 * envelope * (y as f64 - 12.)).sin() * amplitude as f64)
                                .trunc() as i32;
                            if (0..32).contains(&qx) {
                                let a = rgba(qx as usize, y, false);
                                let b = rgba(qx as usize, y, true);
                                for i in 0..4 {
                                    expected[i] = a[i] as f64 * (1. - f) + b[i] as f64 * f;
                                }
                            } else {
                                expected = [
                                    (32. * 256. + 64. * (time * 255 / 100) as f64) / 256.,
                                    (64. * 256. + 64. * (time * 255 / 100) as f64) / 256.,
                                    (96. * 256. - 64. * (time * 255 / 100) as f64) / 256.,
                                    255.,
                                ];
                            }
                        } else if method == "mosaic" {
                            let size = 8 * time.min(100 - time) / 50 + 2;
                            let offset = |w: i32| {
                                let o = (w - size) / 2 - (w / 2 / size) * size;
                                if o > 0 { o - size } else { o }
                            };
                            qx = (((qx - offset(32)) as f64 / size as f64).floor() as i32 * size
                                + offset(32)
                                + size / 2)
                                .clamp(0, 31);
                            qy = (((qy - offset(24)) as f64 / size as f64).floor() as i32 * size
                                + offset(24)
                                + size / 2)
                                .clamp(0, 23);
                            let a = rgba(qx as usize, qy as usize, false);
                            let b = rgba(qx as usize, qy as usize, true);
                            for i in 0..4 {
                                expected[i] = a[i] as f64 * (1. - f) + b[i] as f64 * f;
                            }
                        } else {
                            // Independent box average, including shortened edge windows.
                            let r1 = (4. * time as f64 / 100.).round() as usize;
                            let r2 = (4. * (100 - time) as f64 / 100.).round() as usize;
                            for (second, r, weight) in [(false, r1, 1. - f), (true, r2, f)] {
                                let mut sum = [0u32; 4];
                                let mut count = 0;
                                for yy in y.saturating_sub(r)..=(y + r).min(23) {
                                    for xx in x.saturating_sub(r)..=(x + r).min(31) {
                                        let c = rgba(xx, yy, second);
                                        for i in 0..4 {
                                            sum[i] += c[i] as u32;
                                        }
                                        count += 1;
                                    }
                                }
                                for i in 0..4 {
                                    expected[i] += (sum[i] / count) as f64 * weight;
                                }
                            }
                        }
                        for i in 0..4 {
                            assert!(
                                (pixels[(y * 32 + x) * 4 + i] as f64 - expected[i]).abs() <= 2.1,
                                "{method} restart {restart} time {time} at {x},{y} channel {i}: {} != {}",
                                pixels[(y * 32 + x) * 4 + i],
                                expected[i]
                            );
                        }
                    }
                }
            }
        }
        run(r#"
            d.visible=true; s.visible=false; global.tick=0;
            for(var yy=0; yy<24; yy++) for(var xx=0; xx<32; xx++) {
                var color=0xff000000 | (xx*7 << 16) | (yy*10 << 8) | xx*3;
                d.fillRect(xx,yy,1,1,color); s.fillRect(xx,yy,1,1,color);
            }
            d.beginTransition("blurfade",false,s,%[time:100,selfupdate:true,callback:function(){return tick;},blur1:4,blur2:4,type:1,exponent:2]);
        "#);
        frame();
        run("global.tick=50;");
        let pixels = frame();
        for y in 0..24usize {
            for x in 0..32usize {
                let mut expected = [0.; 3];
                for (radius, weight) in [(1usize, 191. / 255.), (3usize, 64. / 255.)] {
                    let sample = |v: usize, high: usize| {
                        let stride = radius * 2 + 1;
                        let a = v / stride * stride;
                        let b = (a + stride).min(high);
                        if a == b {
                            a as f64
                        } else {
                            a as f64 + (b - a) as f64 * (v - a) as f64 / (b - a) as f64
                        }
                    };
                    expected[0] += sample(x, 31) * 7. * weight;
                    expected[1] += sample(y, 23) * 10. * weight;
                    expected[2] += sample(x, 31) * 3. * weight;
                }
                for i in 0..3 {
                    assert!((pixels[(y * 32 + x) * 4 + i] as f64 - expected[i]).abs() <= 2.1);
                }
            }
        }
        run("global.tick=100;");
        frame();
        for method in ["wave", "mosaic", "blurfade"] {
            run(&format!(
                r#"
                base.type=ltAlpha; base.fillRect(0,0,32,24,0);
                d.visible=true; s.visible=false; d.type=ltAlpha; s.type=ltAlpha;
                d.fillRect(0,0,32,24,0x80ff0000); s.fillRect(0,0,32,24,0x4000ff00);
                global.tick=0;
                d.beginTransition("{method}",false,s,%[time:100,selfupdate:true,callback:function(){{return tick;}},maxh:0,maxsize:4,blur1:2,blur2:2]);
            "#
            ));
            frame();
            run("global.tick=50;");
            let pixels = frame();
            let expected = if method == "wave" {
                [171u8, 84, 0, 96]
            } else if method == "blurfade" {
                [127, 128, 0, 96]
            } else {
                [128, 127, 0, 96]
            };
            for (a, b) in pixels[(12 * 32 + 16) * 4..][..4].iter().zip(expected) {
                assert!(
                    a.abs_diff(b) <= 2,
                    "{method} alpha pixel: {:?}",
                    &pixels[(12 * 32 + 16) * 4..][..4]
                );
            }
            run("global.tick=100;");
            frame();
        }
        for method in ["turn", "rotatezoom", "rotatevanish", "rotateswap", "ripple", "book", "morphing"] {
            run(&format!(r#"
                d.type=ltOpaque; s.type=ltOpaque; d.visible=true; s.visible=false;
                d.fillRect(0,0,32,24,0xffcc2040); s.fillRect(0,0,32,24,0xff20a0d0);
                global.tick=0;
                d.beginTransition("{method}",false,s,%[time:100,selfupdate:true,callback:function(){{return tick;}},centerx:16,centery:12,rwidth:16,roundness:1,speed:6,maxdrift:4]);
            "#));
            frame();
            run("global.tick=50;");
            let middle = frame();
            assert!(middle.chunks_exact(4).any(|pixel| pixel[0] != 0 || pixel[1] != 0 || pixel[2] != 0), "{method} rendered a blank frame");
            run("global.tick=100;");
            let final_frame = frame();
            assert_eq!(&final_frame[(12 * 32 + 16) * 4..][..4], &[32, 160, 208, 255], "{method} final frame");
            run("d.stopTransition();");
        }
        for prerender in [0, 1, 2] {
            run("d.stopTransition();");
            run(&format!(r#"global.tick=0; d.beginTransition("blurfade",false,s,%[time:100,prerender:{prerender},selfupdate:true,callback:function(){{return tick;}}]);"#));
            frame();
            run("global.tick=50;");
            assert!(!frame().is_empty());
            run("global.tick=100;");
            frame();
            run("d.stopTransition();");
        }
        run(
            r#"global.bad=0; try { d.beginTransition("wave",false,s,%[time:100,wavetype:9]); } catch(e) { bad=1; }"#,
        );
        assert_eq!(value("bad"), 1);
        run(
            r#"global.tick=0; d.beginTransition("mosaic",false,s,%[time:100]); d.stopTransition(); w.visible=false;"#,
        );
        drop(engine);
        unsafe { sys::krkr_engine_shutdown() };
    }
}
