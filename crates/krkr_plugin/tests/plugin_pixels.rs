use bevy::{
    app::PluginsState,
    prelude::*,
    render::renderer::{RenderDevice, RenderQueue},
    window::ExitCondition,
};

fn initialize_renderer() {
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: None,
                exit_condition: ExitCondition::DontExit,
                ..default()
            })
            .disable::<bevy::winit::WinitPlugin>(),
    );
    while app.plugins_state() != PluginsState::Ready {
        bevy::tasks::tick_global_task_pools_on_main_thread();
        std::thread::yield_now();
    }
    app.finish();
    app.cleanup();
    krkr_plugin::render::initialize(
        app.world().resource::<RenderDevice>().clone(),
        app.world().resource::<RenderQueue>().clone(),
    );
}

fn run(engine: &krkr_script::Tjs2Engine, source: &str) {
    engine.exec_script(source, "GPU plugin pixels").unwrap();
}

fn integer(engine: &krkr_script::Tjs2Engine, expression: &str) -> i64 {
    engine.eval(expression).unwrap().parse().unwrap()
}

#[test]
fn gpu_plugin_pixel_operations() {
    initialize_renderer();
    let path = std::ffi::CString::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/game")).unwrap();
    assert_eq!(unsafe { krkr_sys::krkr_engine_init(path.as_ptr()) }, krkr_sys::KRKR_OK);
    let engine = krkr_script::Tjs2Engine::new().unwrap();
    run(&engine, r#"
        Plugins.link("layerExBTOA.dll");
        Plugins.link("layerExAreaAverage.dll");
        Plugins.link("shrinkCopy.dll");
        var window = new Window();
        global.source = new Layer(window, null); source.setImageSize(5, 3);
        global.target = new Layer(window, source); target.setImageSize(5, 3);
        source.fillRect(0, 0, 5, 3, 0xff204060);
        source.fillRect(2, 0, 3, 3, 0xff80a0c0);
        target.shrinkCopyFast(source, 2, 2);
    "#);
    assert_eq!(integer(&engine, "target.imageWidth"), 3);
    assert_eq!(integer(&engine, "target.imageHeight"), 2);
    assert_eq!(integer(&engine, "target.getMainPixel(0,0) & 0xffffff"), 0x604020);
    assert_eq!(integer(&engine, "target.getMainPixel(2,1) & 0xffffff"), 0xc0a080);
    run(&engine, r#"
        source.setImageSize(4, 4); source.fillRect(0, 0, 4, 4, 0xff204060);
        source.fillRect(2, 0, 2, 4, 0xff80a0c0);
        target.setImageSize(3, 3); target.fillRect(0, 0, 3, 3, 0xff010203);
        target.shrinkCopy(0, 0, 1, 1, source, 0, 0, 4, 4);
    "#);
    assert_eq!(integer(&engine, "target.getMainPixel(0,0) & 0xffffff"), 0x907050);
    assert_eq!(integer(&engine, "target.getMainPixel(2,2) & 0xffffff"), 0x030201);
    run(&engine, r#"
        source.fillRect(0, 0, 4, 4, 0xffff0000);
        source.fillRect(2, 0, 2, 4, 0x000000ff);
        target.stretchCopyAA(0,0,1,1,source,0,0,4,4);
    "#);
    assert_eq!(integer(&engine, "target.getMainPixel(0,0) & 0xffffff"), 0x0000ff);
    assert_eq!(integer(&engine, "target.getMaskPixel(0,0)"), 170);
    run(&engine, "source.setImageSize(1,1); source.fillRect(0,0,1,1,0xffffffff); target.fillRect(0,0,1,1,0xff102030); target.stretchCopyAA(0,0,1,1,source,0,0,1,1);");
    assert_eq!(integer(&engine, "target.getMainPixel(0,0) & 0xffffff"), 0x302010);
    run(&engine, r#"
        source.setImageSize(4,4);
        target.setImageSize(4, 4); target.fillRect(0,0,4,4,0x80204060);
        target.fillRect(2,0,2,4,0xff1020c0);
        target.copyRightBlueToLeftAlpha();
    "#);
    assert_eq!(integer(&engine, "target.getMaskPixel(0,0)"), 192);
    assert_eq!(integer(&engine, "target.getMaskPixel(3,0)"), 255);
    run(&engine, "target.copyAlphaToProvince();");
    assert_eq!(integer(&engine, "target.getProvincePixel(0,0)"), 192);
    assert_eq!(integer(&engine, "target.getProvincePixel(3,0)"), 255);
    run(&engine, "target.copyAlphaToProvince(200); target.fillByProvince(1, 0xff112233);");
    assert_eq!(integer(&engine, "target.getProvincePixel(0,0)"), 0);
    assert_eq!(integer(&engine, "target.getProvincePixel(3,0)"), 1);
    assert_eq!(integer(&engine, "target.getMainPixel(3,0) & 0xffffff"), 0x332211);
    run(&engine, r#"
        source.fillRect(0,0,4,4,0x80000000);
        target.fillAlpha(); target.clipAlphaRect(1,1,source,0,0,2,2,64);
    "#);
    assert_eq!(integer(&engine, "target.getMaskPixel(0,0)"), 64);
    assert_eq!(integer(&engine, "target.getMaskPixel(1,1)"), 128);
    run(&engine, "target.fillRect(0,2,4,2,0xff000099); target.copyBottomBlueToTopAlpha();");
    assert_eq!(integer(&engine, "target.getMaskPixel(0,0)"), 153);
}
