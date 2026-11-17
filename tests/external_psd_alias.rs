#[cfg(target_os = "linux")]
#[test]
#[ignore = "requires tools/external-plugins/build.sh psdfile"]
fn psd_alias_shares_registration_and_unloads_after_both_names() {
    use bevy_krkr::{script::Tjs2Engine, sys};
    use std::{ffi::CString, os::unix::fs::symlink, path::Path};

    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let built = root.join("target/external-plugins/psdfile");
    assert_eq!(
        std::fs::read_link(built.join("libpsd.so")).unwrap(),
        Path::new("libpsdfile.so")
    );
    let project = std::env::temp_dir().join(format!("krkr-psd-alias-{}", std::process::id()));
    std::fs::create_dir_all(&project).unwrap();
    std::fs::copy(built.join("libpsdfile.so"), project.join("libpsdfile.so")).unwrap();
    symlink("libpsdfile.so", project.join("libpsd.so")).unwrap();
    let path = CString::new(project.to_str().unwrap()).unwrap();
    let fixture = root.join("tests/fixtures/external_probe.psd");

    for names in [["psd.dll", "psdfile.dll"], ["psdfile.dll", "psd.dll"]] {
        assert_eq!(
            unsafe { sys::krkr_engine_init(path.as_ptr()) },
            sys::KRKR_OK
        );
        let engine = Tjs2Engine::new().unwrap();
        for _ in 0..2 {
            engine
                .exec_script(
                    &format!(
                        "Plugins.link({0:?});global.psdClass=PSD;Plugins.link({1:?});Plugins.link({0:?});Plugins.link({1:?});if(PSD!==psdClass) throw 'PSD registered twice';psdClass=void;",
                        names[0], names[1]
                    ),
                    "PSD aliases",
                )
                .unwrap();
            assert_eq!(
                engine
                    .eval(&format!("Plugins.unlink({:?})", names[0]))
                    .unwrap(),
                "1"
            );
            engine
                .exec_script(
                    &format!(
                        "global.psd=new PSD();if(!psd.load({:?})) throw 'PSD load failed';",
                        fixture.to_str().unwrap()
                    ),
                    "PSD remaining alias",
                )
                .unwrap();
            assert_eq!(
                engine
                    .eval(
                        "psd.width+','+psd.height+','+psd.layer_count+','+psd.getLayerInfo(0).name"
                    )
                    .unwrap(),
                "2,2,1,probe"
            );
            assert_eq!(
                engine
                    .eval("Storages.isExistentStorage('psd://external_probe.psd/root/probe.bmp')")
                    .unwrap(),
                "1"
            );
            assert_eq!(
                engine
                    .eval(&format!("Plugins.unlink({:?})", names[1]))
                    .unwrap(),
                "0"
            );
            engine
                .exec_script(
                    "PSD.clearStorageCache();invalidate psd;psd=void;",
                    "PSD cleanup",
                )
                .unwrap();
            assert_eq!(
                engine
                    .eval(&format!("Plugins.unlink({:?})", names[1]))
                    .unwrap(),
                "1"
            );
            assert_eq!(engine.eval("typeof global.PSD").unwrap(), "undefined");
        }
        drop(engine);
        unsafe { sys::krkr_engine_shutdown() };
    }
    std::fs::remove_dir_all(project).unwrap();
}
