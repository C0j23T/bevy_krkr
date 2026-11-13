use bevy_krkr::{
    script::{Tjs2Engine, last_error},
    sys,
};
use std::{ffi::CString, path::Path};

fn chunk(name: &[u8], bytes: &[u8]) -> Vec<u8> {
    [name, &(bytes.len() as u64).to_le_bytes(), bytes].concat()
}
fn archive(name: &str, script: &[u8]) -> Vec<u8> {
    let mut data = b"XP3\r\n \n\x1a\x8b\x67\x01".to_vec();
    data.extend_from_slice(&(19u64 + script.len() as u64).to_le_bytes());
    data.extend_from_slice(script);
    let name: Vec<u8> = name.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let size = script.len() as u64;
    let info = [
        &0u32.to_le_bytes()[..],
        &size.to_le_bytes(),
        &size.to_le_bytes(),
        &((name.len() / 2) as u16).to_le_bytes(),
        &name,
    ]
    .concat();
    let segment = [
        &0u32.to_le_bytes()[..],
        &19u64.to_le_bytes(),
        &size.to_le_bytes(),
        &size.to_le_bytes(),
    ]
    .concat();
    let index = chunk(
        b"File",
        &[
            chunk(b"info", &info),
            chunk(b"segm", &segment),
            chunk(b"adlr", &0u32.to_le_bytes()),
        ]
        .concat(),
    );
    data.push(0);
    data.extend_from_slice(&(index.len() as u64).to_le_bytes());
    data.extend(index);
    data
}
fn run(project: &Path, save: &Path) {
    let name = CString::new(project.to_str().unwrap()).unwrap();
    assert_eq!(
        unsafe { sys::krkr_engine_init(name.as_ptr()) },
        sys::KRKR_OK,
        "{}",
        last_error()
    );
    let engine = Tjs2Engine::new().unwrap();
    assert_eq!(engine.eval("Font.defaultFaceName").unwrap(), "Fira Mono");
    assert_eq!(engine.eval("Storages.isExistentStorage('')").unwrap(), "0");
    engine
        .exec_script("Scripts.execStorage('mixedcase.tjs');", "storage")
        .unwrap();
    assert_eq!(engine.eval("storageProbe").unwrap(), "42");
    engine
        .exec_script(
            "Plugins.link('fstat.dll'); Plugins.link('dirlist.dll');",
            "plugins",
        )
        .unwrap();
    let source = save.join("source.bin");
    let copy = save.join("copy.bin");
    let moved = save.join("moved.bin");
    std::fs::write(&source, b"source").unwrap();
    let q = |p: &Path| format!("{:?}", p.to_str().unwrap());
    assert_eq!(
        engine
            .eval(&format!(
                "Storages.copyFile({}, {}, true)",
                q(&source),
                q(&copy)
            ))
            .unwrap(),
        "1"
    );
    assert_eq!(
        engine
            .eval(&format!(
                "Storages.copyFile({}, {}, true)",
                q(&source),
                q(&copy)
            ))
            .unwrap(),
        "0"
    );
    assert_eq!(
        engine
            .eval(&format!("Storages.moveFile({}, {})", q(&copy), q(&moved)))
            .unwrap(),
        "1"
    );
    assert_eq!(
        engine
            .eval(&format!("Storages.fstat({}).size", q(&moved)))
            .unwrap(),
        "6"
    );
    assert_eq!(
        engine
            .eval(&format!("Storages.deleteFile({})", q(&moved)))
            .unwrap(),
        "1"
    );
    let directory = format!("{}/new-dir/", save.display());
    assert_eq!(
        engine
            .eval(&format!("Storages.createDirectory({directory:?})"))
            .unwrap(),
        "1"
    );
    assert_eq!(
        engine
            .eval(&format!("Storages.isExistentDirectory({directory:?})"))
            .unwrap(),
        "1"
    );
    assert_eq!(
        engine
            .eval(&format!("Storages.removeDirectory({directory:?})"))
            .unwrap(),
        "1"
    );
    assert_ne!(
        engine
            .eval(&format!(
                "getDirList({:?}).count",
                format!("{}/", save.display())
            ))
            .unwrap(),
        "0"
    );
    let file = save.join("中文存档.txt");
    engine
        .exec_script(
            &format!(
                "[\"保存测试\",\"second\"].save({:?});",
                file.to_str().unwrap()
            ),
            "save",
        )
        .unwrap();
    assert_eq!(
        engine
            .eval(&format!(
                "(new Array()).load({:?})[0]",
                file.to_str().unwrap()
            ))
            .unwrap(),
        "保存测试"
    );
    engine.exec_script(&format!("Debug.logLocation = {:?}; Debug.startLogToFile(true); Debug.message('rust storage log');", save.to_str().unwrap()), "log").unwrap();
    let log = std::fs::read(save.join("krkr.console.log")).unwrap();
    let expected: Vec<u8> = "rust storage log"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    assert!(log.windows(expected.len()).any(|bytes| bytes == expected));
    drop(engine);
    unsafe {
        sys::krkr_engine_shutdown();
    }
}
#[test]
fn directory_archive_save_and_log_use_host_storage() {
    sys::font::set_fallback_font(bevy::text::DEFAULT_FONT_DATA);
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "krkr-storage-engine-{}-{unique}",
        std::process::id()
    ));
    std::fs::create_dir(&root).unwrap();
    let data = root.join("Data");
    std::fs::create_dir(&data).unwrap();
    let script = b"global.storageProbe = 42;";
    std::fs::write(data.join("MixedCase.tjs"), script).unwrap();
    let xp3 = root.join("data.xp3");
    std::fs::write(&xp3, archive("MixedCase.tjs", script)).unwrap();
    run(&data, &root);
    run(&xp3, &root);
    std::fs::remove_dir_all(root).unwrap();
}
