mod common;

use bevy_krkr::{script::Tjs2Engine, sys};
use std::ffi::CString;

#[test]
#[ignore = "requires tools/external-plugins/build.sh gpu_fixture"]
fn independent_gpu_plugin_registers_draws_and_unloads() {
    common::initialize_renderer();
    let project = std::env::temp_dir().join(format!("krkr-external-gpu-{}", std::process::id()));
    std::fs::create_dir_all(project.join("plugin")).unwrap();
    std::fs::copy(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/target/external-plugins/gpu_fixture/libgpu_fixture.so"
        ),
        project.join("plugin/libgpu_fixture.so"),
    )
    .unwrap();
    let path = CString::new(project.to_str().unwrap()).unwrap();
    assert_eq!(
        unsafe { sys::krkr_engine_init(path.as_ptr()) },
        sys::KRKR_OK
    );
    let engine = Tjs2Engine::new().unwrap();
    let run = |source: &str| engine.exec_script(source, "external GPU").unwrap();
    run(r#"
        Plugins.link("gpu_fixture.dll");
        global.window = new Window(); window.setInnerSize(4,2); window.visible=true;
        global.source = new Layer(window,null); source.setImageSize(4,2);
        global.target = new Layer(window,source); target.setImageSize(4,2);
        source.fillRect(0,0,4,2,0x804060a0);
        target.fillRect(0,0,4,2,0xff010203);
        gpuFixtureApply(target,source);
    "#);
    let opcode = engine
        .eval("gpuFixtureOpcode")
        .unwrap()
        .parse::<u32>()
        .unwrap();
    assert!(opcode > 100);
    assert_eq!(engine.eval("target.getMainPixel(1,0)").unwrap(), "8413344");
    assert_eq!(engine.eval("target.getMaskPixel(1,0)").unwrap(), "128");
    assert_eq!(engine.eval("target.getMainPixel(0,0)").unwrap(), "197121");
    assert_eq!(engine.eval("target.getMainPixel(3,1)").unwrap(), "197121");
    run("gpuFixtureApply(target,target);");
    assert_eq!(engine.eval("target.getMainPixel(1,0)").unwrap(), "8389123");
    run("global.savedGPUCallback=gpuFixtureApply;");
    assert_eq!(
        engine.eval("Plugins.unlink('gpu_fixture.dll')").unwrap(),
        "0"
    );
    run("global.savedGPUCallback=void;");
    assert_eq!(
        engine.eval("Plugins.unlink('gpu_fixture.dll')").unwrap(),
        "1"
    );
    assert!(engine.eval("gpuFixtureApply").is_err());
    run("Plugins.link('gpu_fixture.dll'); gpuFixtureApply(target,source);");
    let reloaded = engine
        .eval("gpuFixtureOpcode")
        .unwrap()
        .parse::<u32>()
        .unwrap();
    assert!(reloaded > 100);
    assert_ne!(reloaded, opcode);
    assert_eq!(engine.eval("target.getMainPixel(1,0)").unwrap(), "8413344");
    assert_eq!(
        engine.eval("Plugins.unlink('gpu_fixture.dll')").unwrap(),
        "1"
    );
    drop(engine);
    unsafe { sys::krkr_engine_shutdown() };
    std::fs::remove_dir_all(project).unwrap();
}
