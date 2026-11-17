use bevy_krkr::{script::Tjs2Engine, sys};
use std::ffi::CString;

#[test]
#[ignore = "requires tools/external-plugins/build.sh json"]
fn json_storage_roundtrip_and_unload() {
    let project = std::env::temp_dir().join(format!("krkr-external-json-{}", std::process::id()));
    std::fs::create_dir_all(project.join("plugin")).unwrap();
    std::fs::copy(
        concat!(env!("CARGO_MANIFEST_DIR"), "/target/external-plugins/json/libjson.so"),
        project.join("plugin/libjson.so"),
    )
    .unwrap();
    std::fs::write(project.join("input.json"), r#"{"text":"日本語","array":[1,2,3]}"#).unwrap();
    let path = CString::new(project.to_str().unwrap()).unwrap();
    assert_eq!(unsafe { sys::krkr_engine_init(path.as_ptr()) }, sys::KRKR_OK);
    let engine = Tjs2Engine::new().unwrap();
    let run = |source: &str| engine.exec_script(source, "external json").unwrap();
    run(r#"Plugins.link("json.dll"); Plugins.link("json.dll");"#);
    run(r#"global.data=Scripts.evalJSON('{"number":42,"text":"a\\nb","array":[1,2,3]}');"#);
    assert_eq!(engine.eval("data.number").unwrap(), "42");
    assert_eq!(engine.eval("data.array.count").unwrap(), "3");
    assert_eq!(engine.eval("data.text").unwrap(), "a\nb");
    run(r#"global.unicode=Scripts.evalJSON('{"value":"\\u65e5\\u672c"}');"#);
    assert_eq!(engine.eval("unicode.value").unwrap(), "日本");
    run("global.roundtrip=Scripts.evalJSON(Scripts.toJSONString(data));");
    assert_eq!(engine.eval("roundtrip.array[2]").unwrap(), "3");
    run(r#"global.stored=Scripts.evalJSONStorage("input.json",true);"#);
    assert_eq!(engine.eval("stored.text").unwrap(), "日本語");
    run(r#"Scripts.saveJSON("output.json",stored); global.saved=Scripts.evalJSONStorage("output.json");"#);
    assert_eq!(engine.eval("saved.text").unwrap(), "日本語");
    assert!(project.join("output.json").is_file());
    run(r#"Scripts.saveJSON("utf8.json",stored,true);"#);
    assert!(std::fs::read_to_string(project.join("utf8.json")).unwrap().contains("日本語"));
    run("global.failed=false; try { Scripts.evalJSON('{'); } catch(e) { failed=true; }");
    assert_eq!(engine.eval("failed").unwrap(), "1");
    assert_eq!(engine.eval("Plugins.unlink('json.dll')").unwrap(), "1");
    for method in ["evalJSON", "evalJSONStorage", "saveJSON", "toJSONString"] {
        assert_ne!(engine.eval(&format!("typeof Scripts.{method}")).unwrap(), "Object");
    }
    run("Plugins.link('json.dll');");
    assert_eq!(engine.eval("Scripts.evalJSON('[7]')[0]").unwrap(), "7");
    drop(engine);
    unsafe { sys::krkr_engine_shutdown() };
    std::fs::remove_dir_all(project).unwrap();
}
