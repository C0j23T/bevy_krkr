mod common;

use bevy_krkr::{script::Tjs2Engine, sys};
use std::ffi::CString;

#[test]
fn geometry_path_transform_alpha_and_stroke() {
    common::initialize_renderer();
    let path=CString::new(format!("{}/assets/game",env!("CARGO_MANIFEST_DIR"))).unwrap();
    assert_eq!(unsafe { sys::krkr_engine_init(path.as_ptr()) },sys::KRKR_OK);
    let engine=Tjs2Engine::new().unwrap();
    let run=|s:&str| engine.exec_script(s,"LayerExDraw GPU").unwrap();
    let value=|s:&str| engine.eval(s).unwrap().parse::<i64>().unwrap();
    run(r#"
        Plugins.link("layerExDraw.dll");
        global.p=new GdiPlus.PointF(2,3); p.x=4;
        global.r=new GdiPlus.RectF(1,2,4,5); global.copy=r.Clone();
        r.Inflate(1,2); r.Offset(2,1);
        global.matrix=new GdiPlus.Matrix();
        matrix.Translate(3,4); matrix.Scale(2,3,GdiPlus.MatrixOrderAppend);
    "#);
    assert_eq!(value("p.Equals([4,3])"),1);
    assert_eq!(value("copy.Equals([1,2,4,5])"),1);
    assert_eq!(value("r.Equals([2,1,6,9])"),1);
    assert_eq!(value("matrix.OffsetX()*10+matrix.OffsetY()"),72);
    run("matrix.Invert(); matrix.Multiply([2,0,0,3,6,12],GdiPlus.MatrixOrderAppend);");
    assert_eq!(value("matrix.IsIdentity()"),1);
    run("matrix.SetElements(0,0,0,0,1,2);");
    assert_eq!(value("matrix.Invert()"),2);
    assert_eq!(value("matrix.GetLastStatus()"),2);
    assert_eq!(value("matrix.GetLastStatus()"),0);
    run(r#"
        global.w=new Window(); w.setInnerSize(32,32);
        global.s=new Layer(w,null); s.setImageSize(32,32); s.type=ltAlpha; s.clear(0);
        global.app=new GdiPlus.Appearance(); app.addBrush(0x80ff0000);
        global.bounds=s.drawRectangle(app,2,2,8,8);
    "#);
    assert!((value("s.getMaskPixel(5,5)")-128).abs()<=1);
    assert_eq!(value("s.getMainPixel(5,5)"),255);
    assert_eq!(value("bounds.width*10+bounds.height"),88);
    run(r#"
        app.clear(); app.addBrush(0x800000ff); s.drawRectangle(app,2,2,8,8);
    "#);
    assert!((value("s.getMaskPixel(5,5)")-192).abs()<=1);
    let pixel=value("s.getMainPixel(5,5)");
    assert!(((pixel&255)-85).abs()<=1);
    assert!((((pixel>>16)&255)-170).abs()<=1);
    run(r#"
        s.clear(0); app.clear(); app.addBrush(0xff00ff00);
        global.path=new GdiPlus.Path(); path.drawRectangle(1,1,20,20); path.drawRectangle(5,5,8,8);
        s.drawPath(app,path);
    "#);
    assert_eq!(value("s.getMaskPixel(2,2)"),255);
    assert_eq!(value("s.getMaskPixel(7,7)"),0);
    run(r#"
        s.clear(0); s.scaleTransform(2,2); s.translateTransform(3,4); s.translateViewTransform(1,2);
        bounds=s.drawRectangle(app,1,1,3,2);
    "#);
    assert_eq!(value("s.getMaskPixel(7,9)"),255);
    assert_eq!(value("s.getMaskPixel(5,9)"),0);
    assert_eq!(value("bounds.x*1000+bounds.y*100+bounds.width*10+bounds.height"),6864);
    run(r#"
        s.resetTransform(); s.resetViewTransform(); s.clear(0); app.clear();
        app.addPen(0xffffffff,%[width:2,startCap:2,endCap:2,lineJoin:2]);
        s.drawLine(app,5,10,20,10);
    "#);
    assert_eq!(value("s.getMaskPixel(10,10)"),255);
    assert_eq!(value("s.getMaskPixel(10,13)"),0);
    run(r#"
        s.clear(0); app.clear(); app.addBrush(%[type:4,color1:0xffff0000,color2:0xff0000ff,point1:[0,0],point2:[20,0],wrapMode:4]);
        s.setClip(4,4,12,12); s.drawEllipse(app,0.5,0.5,20,20);
    "#);
    assert_eq!(value("s.getMaskPixel(3,10)"),0);
    assert!(value("s.getMaskPixel(10,10)")>250);
    run(r#"
        s.setClip(0,0,32,32); s.clear(0); app.clear(); app.addPen(0xffffffff,1);
        path=new GdiPlus.Path(); path.drawArc(1,1,20,10,45,90); path.startFigure();
        path.drawCurve([[1,20],[10,12],[20,24]]); path.closeFigure(); bounds=s.drawPath(app,path);
    "#);
    assert!(value("int(bounds.width)")>0);
    run(r#"
        s.resetTransform(); s.resetViewTransform(); s.clear(0);
        global.src=new Layer(w,s); src.setImageSize(4,4); src.type=ltAlpha; src.clear(0xffff0000);
        src.fillRect(2,0,2,4,0xff0000ff);
        s.drawImageStretch(4,4,8,8,src,0,0,4,4);
    "#);
    assert_eq!(value("s.getMainPixel(5,6)"),255);
    assert_eq!(value("s.getMainPixel(10,6)"),0xff0000);
    run("s.clear(0); s.drawImageRect(5,6,src,2,0,2,4);");
    assert_eq!(value("s.getMainPixel(5,6)"),0xff0000);
    assert_eq!(value("s.getMaskPixel(7,6)"),0);
    run("s.clear(0); s.drawImageAffine(src,0,0,4,4,false,3,4,11,4,3,12);");
    assert_eq!(value("s.getMainPixel(4,6)"),255);
    assert_eq!(value("s.getMainPixel(9,6)"),0xff0000);
    run(r#"
        s.clear(0); global.font=new GdiPlus.Font("DejaVu Sans",16,0);
        app.clear(); app.addPen(0xff0000ff,2); app.addBrush(0xffffffff);
        bounds=s.drawPathString(font,app,2,1,"Ag");
    "#);
    assert!(value("int(bounds.width)")>10);
    let mut ink=0;
    for y in 0..24 { for x in 0..30 { ink+=value(&format!("s.getMaskPixel({x},{y})")); } }
    assert!(ink>1000);
    run("s.clear(0); app.clear(); app.addBrush(0xffffffff); s.translateTransform(10,0); s.drawString(font,app,2,1,'Ag');");
    let mut left=0;
    for y in 0..24 { for x in 0..10 { left+=value(&format!("s.getMaskPixel({x},{y})")); } }
    assert_eq!(left,0);
    run(r#"
        s.resetTransform(); s.record=true; s.clear(0); app.clear(); app.addBrush(0xff00ff00);
        s.drawRectangle(app,2,3,4,5); app.clear();
        s.translateViewTransform(8,0);
    "#);
    assert_eq!(value("s.getMaskPixel(3,4)"),0);
    assert_eq!(value("s.getMainPixel(11,4)"),0x00ff00);
    run("global.recorded=s.getRecordImage(); s.record=false; s.resetViewTransform(); s.clear(0); s.drawImage(0,0,recorded);");
    assert_eq!(value("s.getMainPixel(3,4)"),0x00ff00);
    assert_eq!(value("recorded.GetType()"),2);
    run("s.clear(0); app.clear(); app.addBrush(0xff402010); s.drawRectangle(app,1,2,3,4); global.regions=s.getColorRegionRects(0xff402010);");
    assert_eq!(value("regions.count"),1);
    assert_eq!(engine.eval("regions[0].join(',')").unwrap(),"1,2,3,4");
    let saved=std::env::temp_dir().join(format!("krkr-layerexdraw-{}.png",std::process::id()));
    run(&format!("s.saveImage('{}','image/png'); global.loaded=new GdiPlus.Image('{}'); s.clear(0); s.drawImage(0,0,loaded);",saved.display(),saved.display()));
    assert_eq!(value("s.getMainPixel(2,3)"),0x102040);
    std::fs::remove_file(saved).unwrap();
    run(r#"
        app.clear(); app.addBrush(%[type:1,hatchStyle:4,foreColor:0xffff0000,backColor:0xff0000ff]);
        s.clear(0); s.drawRectangle(app,0,0,16,16);
    "#);
    assert_eq!(value("s.getMaskPixel(2,2)"),255);
    let mut red=0;
    let mut blue=0;
    for y in 0..8 { for x in 0..8 {
        let pixel=value(&format!("s.getMainPixel({x},{y})"));
        red+=i64::from(pixel==255); blue+=i64::from(pixel==0xff0000);
    } }
    assert!(red>0 && blue>0);
    run(r#"
        app.clear(); app.addBrush(%[type:3,points:[[0,0],[20,0],[20,20],[0,20]],centerPoint:[10,10],centerColor:0xffffffff,surroundColors:[0xff0000ff]]);
        s.clear(0); s.drawRectangle(app,0,0,20,20);
    "#);
    let center=value("s.getMainPixel(10,10)");
    let edge=value("s.getMainPixel(19,10)");
    assert!((center&255)>(edge&255));
    run(r#"
        app.clear(); app.addBrush(%[type:4,rect:[0,0,8,8],mode:2,color1:0xffff0000,color2:0xff0000ff,wrapMode:1]);
        s.clear(0); s.drawRectangle(app,0,0,24,24);
    "#);
    assert_ne!(value("s.getMainPixel(3,3)"),value("s.getMainPixel(11,3)"));
    run(r#"
        s.clear(0); app.clear(); app.addPen(0xffffffff,%[width:2,startCap:%[width:4,height:5,filled:true],endCap:2]);
        s.drawLine(app,8,16,24,16);
    "#);
    assert_eq!(value("s.getMaskPixel(16,16)"),255);
    run("s.clear(0); app.clear(); app.addPen(0xffffffff,%[width:4,alignment:1]); s.drawRectangle(app,4,4,20,20);");
    assert_eq!(value("s.getMaskPixel(3,10)"),0);
    assert_eq!(value("s.getMaskPixel(5,10)"),255);
    run("s.clear(0); app.clear(); app.addPen(0xffffffff,%[width:6,compoundArray:[0,0.2,0.8,1]]); s.drawLine(app,4,16,24,16);");
    assert_eq!(value("s.getMaskPixel(12,16)"),0);
    let emf=std::env::temp_dir().join(format!("krkr-layerexdraw-{}.emf",std::process::id()));
    run("s.record=true; s.clear(0); app.clear(); app.addBrush(0xff20a040); s.drawRectangle(app,2,3,6,5);");
    run(&format!("s.saveRecord('{}'); s.record=false; s.clear(0); s.loadRecord('{}');",emf.display(),emf.display()));
    assert_eq!(value("s.getMainPixel(4,5)"),0x40a020);
    run(&format!("global.metafile=new GdiPlus.Image('{}'); s.record=false; s.clear(0); s.drawImageStretch(0,0,64,64,metafile,0,0,32,32);",emf.display()));
    assert_eq!(value("s.getMainPixel(8,10)"),0x40a020);
    run("s.record=true; s.clear(0); s.drawImageStretch(0,0,16,16,metafile,2,3,6,5);");
    run(&format!("s.saveRecord('{}'); s.record=false; s.clear(0); s.loadRecord('{}');",emf.display(),emf.display()));
    assert_eq!(value("s.getMainPixel(10,10)"),0x40a020);
    assert_eq!(value("s.getMaskPixel(18,10)"),0);
    run("s.record=false; s.clear(0);");
    let fixture=concat!(env!("CARGO_MANIFEST_DIR"),"/crates/krkr_plugin/tests/fixtures/layer_ex_draw/wine-mixed.emf");
    run(&format!("s.setImageSize(96,80); s.setClip(0,0,96,80); s.loadRecord('{fixture}');"));
    assert_eq!(value("s.getMainPixel(53,6)"),0x00ff00);
    assert_eq!(value("s.getMainPixel(62,6)"),0x202020);
    assert_eq!(value("s.getMainPixel(79,6)"),0x202020);
    assert_eq!(value("s.getMainPixel(12,2)"),value("s.getMainPixel(12,14)"));
    assert!(value("s.getMainPixel(12,2)") & 255 > 150);
    assert_ne!(value("s.getMainPixel(11,59)"),0x202020);
    run("global.textInk=0; for(var j=21;j<49;++j) for(var i=23;i<75;++i) if(s.getMainPixel(i,j)!=0x202020) ++textInk;");
    assert!(value("textInk")>45);
    std::fs::remove_file(emf).unwrap();
    run("global.rejected=0; try { s.loadRecord('nonexistent.emf'); } catch(e) { rejected=1; }");
    assert_eq!(value("rejected"),1);
    drop(engine);
    unsafe { sys::krkr_engine_shutdown() };
}
