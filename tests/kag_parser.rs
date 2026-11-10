mod common;

use bevy_krkr::{
    script::{Tjs2Engine, last_error},
    sys,
};
use std::ffi::CString;

#[test]
fn extended_parser_and_restart() {
    common::initialize_renderer();
    let path = CString::new(format!("{}/assets/game", env!("CARGO_MANIFEST_DIR"))).unwrap();
    for _ in 0..2 {
        assert_eq!(
            unsafe { sys::krkr_engine_init(path.as_ptr()) },
            sys::KRKR_OK,
            "{}",
            last_error()
        );
        let engine = Tjs2Engine::new().unwrap();
        engine
            .exec_script(
                r#"
            class Scenario extends KAGParser {
                var source = "";
                function Scenario() { super.KAGParser(); }
                function onScenarioLoad(name) { return source; }
            }
            function parse(source) {
                var p = new Scenario(); p.source = source; p.debugLevel = 0;
                p.multiLineTagEnabled = true; p.loadScenario("test.ks"); return p;
            }
        "#,
                "parser setup",
            )
            .unwrap();
        let check = |source: &str, expected: &str| {
            assert_eq!(
                engine
                    .eval(source)
                    .unwrap_or_else(|e| panic!("{source}: {e}")),
                expected,
                "{source}"
            );
        };
        check(
            r#"(function(){ var p=parse("@tag z=1 a=2 z=3"); var t=p.getNextTag(); return t.tagname+":"+t.z+":"+t.taglist.join(","); })()"#,
            "tag:3:tagname,z,a,z",
        );
        check(
            r#"(function(){ var p=parse("@tag x=1 \\\n; y=2 \\\n; z=3\n@next"); var t=p.getNextTag(); return t.x+t.y+t.z+":"+p.getNextTag().tagname; })()"#,
            "123:next",
        );
        check(
            r#"(function(){ var p=parse("[emb exp=\"'[tag x=ok]'\" escape=false]"); return p.getNextTag().x; })()"#,
            "ok",
        );
        check(
            r#"(function(){ var p=parse("[emb exp=\"'[tag]'\"]"); var t=p.getNextTag(); return t.tagname+":"+t.text; })()"#,
            "ch:[",
        );
        check(
            r#"(function(){ var p=parse("@pmacro name=style x=first y=second\n@tag style z=third\n@erasepmacro name=style\n@tag style"); var t=p.getNextTag(); var s=t.x+":"+t.y+":"+t.z+":"+t.taglist.join(","); t=p.getNextTag(); return s+":"+t.style; })()"#,
            "first:second:third:tagname,x,y,z:true",
        );
        check(
            r#"(function(){ var p=parse("[macro name=m][tag before=one * after=three][endmacro][m middle=two]"); var t=p.getNextTag(); return t.before+":"+t.middle+":"+t.after; })()"#,
            "one:two:three",
        );
        check(
            r#"(function(){ var p=parse("@tag style"); p.paramMacros.style=["x","&1+2"]; return p.getNextTag().x; })()"#,
            "3",
        );
        check(
            r#"(function(){ var p=parse("@tag x=1 \\\nnot_a_continuation"); try { p.getNextTag(); } catch(e) { return "error"; } return "accepted"; })()"#,
            "error",
        );
        check(
            r#"(function(){ var p=parse("[macro name=m][tag *][tag *][endmacro][m clear=ok assign=yes middle=two]"); p.getNextTag(); var state=p.store(); var q=new Scenario(); q.source=p.source; q.debugLevel=0; q.restore(state); var t=q.getNextTag(); return t.clear+":"+t.assign+":"+t.middle+":"+q.multiLineTagEnabled; })()"#,
            "ok:yes:two:1",
        );
        check(
            r#"(function(){ var p=parse("@tag style"); p.paramMacros.style=["x","ok"]; var q=new Scenario(); q.source=p.source; q.assign(p); return q.getNextTag().x+":"+q.multiLineTagEnabled; })()"#,
            "ok:1",
        );
        check(
            r#"(function(){ var p=parse("@tag style"); p.paramMacros.style=["style","true"]; try { p.getNextTag(); } catch(e) { return "error"; } return "accepted"; })()"#,
            "error",
        );
        drop(engine);
        unsafe { sys::krkr_engine_shutdown() };
    }
}
