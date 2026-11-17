use bevy_krkr::{script::Tjs2Engine, sys};
use std::ffi::CString;

#[test]
#[ignore = "requires tools/external-plugins/build.sh ExtKAGParser"]
fn extended_kag_parser_features_state_and_restoration() {
    let project = std::env::temp_dir().join(format!("krkr-external-kag-{}", std::process::id()));
    std::fs::create_dir_all(project.join("plugin")).unwrap();
    let plugin = std::env::var("EXT_KAG_PLUGIN").unwrap_or_else(|_| {
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/target/external-plugins/ExtKAGParser/libExtKAGParser.so"
        )
        .into()
    });
    std::fs::copy(plugin, project.join("plugin/libExtKAGParser.so")).unwrap();
    let scenario = concat!(
        "*start|見出し\n日本🙂[[[l]\n",
        "[macro name=greet][emb exp=\"'宏'\"][endmacro]\n[greet]\n",
        "[if exp=\"false\"]無[else]有[endif]\n",
        "[while exp=\"global.counter<3\" each=\"global.counter++\"][emb exp=\"global.counter\"][endwhile]\n",
        "[pushlocalvar value=inner][probe value=$value][poplocalvar]\n",
        "[probe \\\n value=multiline]\n",
        "[pmacro name=style color=blue][probe style]\n",
    );
    let scenario: Vec<u8> = std::iter::once(0xfeffu16)
        .chain(scenario.encode_utf16())
        .flat_map(u16::to_le_bytes)
        .collect();
    std::fs::write(project.join("scenario.ks"), scenario).unwrap();
    let path = CString::new(project.to_str().unwrap()).unwrap();
    assert_eq!(
        unsafe { sys::krkr_engine_init(path.as_ptr()) },
        sys::KRKR_OK
    );
    let engine = Tjs2Engine::new().unwrap();
    let run = |source: &str| engine.exec_script(source, "external ExtKAGParser").unwrap();
    run(r#"
        global.originalParser=KAGParser;
        Plugins.link("ExtKAGParser.dll"); Plugins.link("ExtKAGParser.dll");
        global.parser=new KAGParser();
        parser.ignoreCR=true;
        parser.multiLineTagEnabled=true;
        parser.numericMacroArgumentsEnabled=true;
        global.counter=0;
        parser.loadScenario("scenario.ks");
        global.text=""; global.characters=[]; global.probes=[]; global.lineTags=0;
        for (var guard=0; guard<100; guard++) {
            var tag=parser.getNextTag();
            if (tag===void) break;
            if (tag.tagname=="ch") { text+=tag.text; characters.push(tag.text); }
            else if (tag.tagname=="probe") probes.push(tag.value===void ? tag.color : tag.value);
            else if (tag.tagname=="l") lineTags++;
        }
    "#);
    assert_eq!(engine.eval("text").unwrap(), "日本🙂[宏有012");
    assert_eq!(engine.eval("characters[2]").unwrap(), "🙂");
    assert_eq!(
        engine.eval("probes.join(',')").unwrap(),
        "inner,multiline,blue"
    );
    assert_eq!(engine.eval("lineTags").unwrap(), "1");
    assert_eq!(engine.eval("parser.whileStackDepth").unwrap(), "0");
    assert_eq!(engine.eval("parser.localVariablesDepth").unwrap(), "1");
    assert_eq!(
        engine.eval("Plugins.unlink('ExtKAGParser.dll')").unwrap(),
        "0"
    );
    run(r#"
        parser.loadScenario("scenario.ks");
        parser.getNextTag();
        global.saved=parser.store();
        global.nextText=parser.getNextTag().text;
        parser.restore(saved);
        global.restoredText=parser.getNextTag().text;
        global.copy=new KAGParser(); copy.assign(parser);
        global.copyText=copy.getNextTag().text;
        global.originalText=parser.getNextTag().text;
    "#);
    assert_eq!(engine.eval("restoredText==nextText").unwrap(), "1");
    assert_eq!(engine.eval("copyText==originalText").unwrap(), "1");
    run("invalidate copy; copy=void; invalidate parser; parser=void;");
    assert_eq!(
        engine.eval("Plugins.unlink('ExtKAGParser.dll')").unwrap(),
        "1"
    );
    assert_eq!(engine.eval("KAGParser===originalParser").unwrap(), "1");
    run(
        "Plugins.link('ExtKAGParser.dll'); global.parser=new KAGParser(); parser.onScenarioLoad=function(name){return '再読🙂';}; parser.ignoreCR=true; parser.loadScenario('inline');",
    );
    assert_eq!(engine.eval("parser.getNextTag().text").unwrap(), "再");
    run("invalidate parser; parser=void;");
    assert_eq!(
        engine.eval("Plugins.unlink('ExtKAGParser.dll')").unwrap(),
        "1"
    );
    assert_eq!(engine.eval("KAGParser===originalParser").unwrap(), "1");
    drop(engine);
    unsafe { sys::krkr_engine_shutdown() };
    std::fs::remove_dir_all(project).unwrap();
}
