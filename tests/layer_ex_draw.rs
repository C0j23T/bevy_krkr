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
        path.drawCurve([[1,20],[10,12],[20,24]]); path.closeFigure(); s.drawPath(app,path);
    "#);
    assert!(value("bounds.width")>0);
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
    drop(engine);
    unsafe { sys::krkr_engine_shutdown() };
}
