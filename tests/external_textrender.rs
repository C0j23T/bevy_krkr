mod common;

use bevy_krkr::{script::Tjs2Engine, sys};
use std::ffi::CString;

static ENGINE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
#[ignore = "requires tools/external-plugins/build.sh textrender"]
fn textrender_layout_callbacks_links_scroll_and_reload() {
    let _guard = ENGINE_LOCK.lock().unwrap();
    let project =
        std::env::temp_dir().join(format!("krkr-external-textrender-{}", std::process::id()));
    std::fs::create_dir_all(project.join("plugin")).unwrap();
    let plugin = std::env::var("TEXTRENDER_PLUGIN").unwrap_or_else(|_| {
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/target/external-plugins/textrender/libtextrender.so"
        )
        .into()
    });
    std::fs::copy(plugin, project.join("plugin/libtextrender.so")).unwrap();
    let path = CString::new(project.to_str().unwrap()).unwrap();
    assert_eq!(
        unsafe { sys::krkr_engine_init(path.as_ptr()) },
        sys::KRKR_OK
    );
    let engine = Tjs2Engine::new().unwrap();
    let run = |source: &str| engine.exec_script(source, "external textrender").unwrap();
    let eq = |source: &str, expected: &str| {
        let actual = engine.eval(source).unwrap();
        if let (Ok(actual), Ok(expected)) = (actual.parse::<f64>(), expected.parse::<f64>()) {
            assert_eq!(actual, expected, "{source}");
        } else {
            assert_eq!(actual, expected, "{source}");
        }
    };
    run(r#"
        global.firstFont=new Font(); firstFont.height=20;
        global.initialWidth=firstFont.getTextWidth('日本Ab');
        global.initialHeight=firstFont.getTextHeight('日本Ab');
        invalidate firstFont; firstFont=void;
        Plugins.link('textrender.dll'); Plugins.link('textrender.dll');
        global.r=new TextRenderBase();
        global.fontChanges=0; global.widthCalls=0; global.heightCalls=0;
        r.onFontChange=function(font) { fontChanges++; };
        r.onGetTextWidth=function(text,size) { widthCalls++; return 10; };
        r.onGetTextHeight=function(text,size) { heightCalls++; return 20; };
        r.setDefault(%[fontsize:20,linesize:20,linespacing:2]);
        r.setRenderSize(30,44);
        r.render('日本🙂ABCD',0,5);
        global.ch=r.getCharacters(0,0);
    "#);
    eq("initialWidth>0 && initialHeight==20", "1");
    eq("r.renderText", "日本🙂ABCD");
    eq("r.renderCount", "7");
    eq("ch.count", "7");
    eq("ch[2].text", "🙂");
    eq("ch[2].x", "20");
    eq("ch[3].y", "22");
    eq("ch[6].y", "44");
    eq("r.renderLines", "3");
    eq("r.renderBottom", "66");
    eq("r.renderOver", "1");
    eq("r.maxScrollOffset", "22");
    eq("r.maxScrollLine", "1");
    eq("r.calcLineOffset(1)", "22");
    eq("r.calcShowCount(9)", "2");
    eq("widthCalls==7 && heightCalls==7", "1");
    eq("fontChanges", "1");
    eq("r.contains(1,1)", "1");
    eq("r.contains(-1,1)", "0");
    eq("r.getCharacters(2,2).count", "2");
    eq("r.getCharacters(99,0).count", "0");
    run(r#"
        r.clear(); r.render('%l同名;ABCDE%l;%l同名;Z%l;',0,0);
        global.links=r.getLinkNames(); global.rects=r.getLinkRects(0);
    "#);
    eq("links.join(',')", "同名,同名");
    eq("rects.count", "2");
    eq("rects[0].width", "30");
    eq("rects[1].top", "22");
    eq("rects[1].width", "20");
    eq("r.getLinkCharacters(0).count", "5");
    eq("r.getLinkCharacters(1)[0].text", "Z");
    eq("r.isLinkContains(0,1,23)", "1");
    eq("r.getLinkOfPosition(25,23)", "1");
    eq("r.getLinkOfPosition(100,100)", "-1");
    eq("r.getLinkRects(-1).count", "0");
    run(r#"
        r.onGetGraphSize=function(name) { return %[width:17,height:13]; };
        r.onEval=function(expression) { return '替🙂'; };
        r.onLabel=function(label) { return 30; };
        r.setRenderSize(200,100); r.render('[ルビ]漢#ff0000;字&図🙂;$value;%w$待機;終',0,10);
        global.ch=r.getCharacters(0,0);
    "#);
    eq("r.renderText", "漢字替🙂終");
    eq("ch.count", "6");
    eq("ch[0].ruby[0].text", "ルビ");
    eq("ch[1].color", "16711680");
    eq("ch[2].graph", "1");
    eq("ch[2].text", "図🙂");
    eq("ch[2].cw", "17");
    eq("ch[2].size", "13");
    eq("r.getKeyWait()[0].pos", "5");
    eq("r.getKeyWait()[0].time", "50");
    eq("ch[5].delay", "80");
    run("r.render('次',0,10); global.ch=r.getCharacters(0,0);");
    eq("ch[5].delay", "0");
    eq("ch[6].delay", "0");
    run("r.render('続',0,10,false,true); global.ch=r.getCharacters(0,0);");
    eq("ch[7].delay", "10");
    run(r#"
        r.setOption(%[vertical:true]);
        r.setDefault(%[align:1,valign:-1]); r.setRenderSize(44,40);
        r.render('甲乙丙丁戊',0,0); global.ch=r.getCharacters(0,0);
    "#);
    eq("r.renderLines", "3");
    eq("ch[0].vertical", "1");
    eq("ch[0].x", "34");
    eq("ch[1].y", "20");
    eq("ch[2].x", "12");
    eq("r.renderOver", "1");
    eq("r.maxScrollOffset", "22");
    run(r#"
        r.setOption(%[vertical:false]);
        r.setDefault(%[align:0,valign:0]); r.setRenderSize(100,100);
        r.render('AB',0,0); global.ch=r.getCharacters(0,0);
    "#);
    eq("ch[0].x", "40");
    eq("ch[0].y", "39");
    run(r#"
        r.setDefault(%[align:-1,valign:-1]); r.setRenderSize(20,100);
        r.render('甲乙、丙',0,0); global.ch=r.getCharacters(0,0);
    "#);
    eq("ch[2].y", "0");
    eq("ch[3].y", "22");
    eq("r.renderOver", "0");
    run(r#"
        r.setRenderSize(20,100); r.render('甲（乙丙',0,0);
        global.ch=r.getCharacters(0,0);
    "#);
    eq("ch[1].x", "0");
    eq("ch[1].y", "22");
    run(r#"
        r.setOption(%[word_break:1]); r.setRenderSize(50,100);
        r.render('AB CDE',0,0); global.ch=r.getCharacters(0,0);
    "#);
    eq("ch[3].x", "0");
    eq("ch[3].y", "22");
    run(r#"
        r.fontScale=2; r.setRenderSize(200,100);
        r.render('%b1%i1#12abcd;%200;A',0,0); global.ch=r.getCharacters(0,0);
    "#);
    eq("ch[0].bold && ch[0].italic", "1");
    eq("ch[0].color", "1223629");
    run(r#"
        r.fontScale=1;
        r.setOption(%[ignore_color:true,ignore_size:true,ignore_ruby:true,ignore_delay:true]);
        r.setRenderSize(200,100); r.render('[ルビ]#ff0000;%200;A',0,10);
        global.ch=r.getCharacters(0,0);
    "#);
    eq("ch[0].color", "16777215");
    eq("typeof ch[0].ruby", "undefined");
    eq("r.renderDelay", "0");
    run(r#"
        r.onGetTextWidth=function(text,size) { throw 'measurement failure'; };
        global.callbackFailed=false;
        try { r.render('A',0,0); } catch (e) { callbackFailed=true; }
        r.onGetTextWidth=function(text,size) { return 10; };
        r.clear(); r.render('B',0,0);
    "#);
    eq("callbackFailed", "1");
    eq("r.renderText", "B");
    run(r#"
        global.native=new TextRenderBase(); native.setRenderSize(500,200);
        native.render('日本🙂Ab',0,0); global.measured=native.getCharacters(0,0);
    "#);
    eq("measured.count", "5");
    eq("measured[2].text", "🙂");
    eq(
        "measured[0].cw>0 && measured[0].size>0 && native.renderRight>0",
        "1",
    );
    eq("Plugins.unlink('textrender.dll')", "0");
    run("invalidate native; native=void; invalidate r; r=void;");
    eq("Plugins.unlink('textrender.dll')", "1");
    run("global.removed=false; try { TextRenderBase; } catch(e) { removed=true; }");
    eq("removed", "1");
    run("Plugins.link('textrender.dll'); global.r=new TextRenderBase(); r.setRenderSize(100,100); r.render('再読',0,0);");
    eq("r.renderCount", "2");
    run("invalidate r; r=void;");
    eq("Plugins.unlink('textrender.dll')", "1");
    drop(engine);
    unsafe { sys::krkr_engine_shutdown() };
    std::fs::remove_dir_all(project).unwrap();
}

#[test]
#[ignore = "requires textrender plugin and a Vulkan adapter"]
fn textrender_public_layer_image_dimensions() {
    let _guard = ENGINE_LOCK.lock().unwrap();
    common::initialize_renderer();
    let project =
        std::env::temp_dir().join(format!("krkr-textrender-image-{}", std::process::id()));
    std::fs::create_dir_all(project.join("plugin")).unwrap();
    let plugin = std::env::var("TEXTRENDER_PLUGIN").unwrap_or_else(|_| {
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/target/external-plugins/textrender/libtextrender.so"
        )
        .into()
    });
    std::fs::copy(plugin, project.join("plugin/libtextrender.so")).unwrap();
    let mut bmp = vec![0u8; 54 + 24];
    bmp[0..2].copy_from_slice(b"BM");
    bmp[2..6].copy_from_slice(&78u32.to_le_bytes());
    bmp[10..14].copy_from_slice(&54u32.to_le_bytes());
    bmp[14..18].copy_from_slice(&40u32.to_le_bytes());
    bmp[18..22].copy_from_slice(&3i32.to_le_bytes());
    bmp[22..26].copy_from_slice(&2i32.to_le_bytes());
    bmp[26..28].copy_from_slice(&1u16.to_le_bytes());
    bmp[28..30].copy_from_slice(&24u16.to_le_bytes());
    std::fs::write(project.join("inline.bmp"), bmp).unwrap();
    let path = CString::new(project.to_str().unwrap()).unwrap();
    assert_eq!(
        unsafe { sys::krkr_engine_init(path.as_ptr()) },
        sys::KRKR_OK
    );
    let engine = Tjs2Engine::new().unwrap();
    engine
        .exec_script(
            r#"
        Plugins.link('textrender.dll');
        global.r=new TextRenderBase(); r.setRenderSize(100,100);
        r.render('&inline.bmp;',0,0); global.ch=r.getCharacters(0,0);
    "#,
            "textrender public image fallback",
        )
        .unwrap();
    assert_eq!(engine.eval("ch[0].cw").unwrap(), "3");
    assert_eq!(engine.eval("ch[0].size").unwrap(), "2");
    assert_eq!(engine.eval("ch[0].graph").unwrap(), "1");
    engine
        .exec_script(
            r#"
        r.clear(); r.render('&missing.bmp;',0,0);
        global.missing=r.getCharacters(0,0);
        invalidate r; r=void;
    "#,
            "textrender image failure cleanup",
        )
        .unwrap();
    assert_eq!(engine.eval("missing[0].cw").unwrap(), "1");
    assert_eq!(engine.eval("missing[0].size").unwrap(), "1");
    assert_eq!(
        engine.eval("Plugins.unlink('textrender.dll')").unwrap(),
        "1"
    );
    drop(engine);
    unsafe { sys::krkr_engine_shutdown() };
    std::fs::remove_dir_all(project).unwrap();
}
