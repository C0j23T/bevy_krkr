//! `Scripts.foreach` / `Scripts.getObjectKeys`, which the engine has to
//! provide for the supported KAGeX scripts.
//!
//! KAG's system scripts pass extra arguments including `void`, and pass the result
//! of `getObjectKeys` straight into an argument list with `*`, so both the
//! enumeration and the empty/non-empty cases have to behave like the original:
//! `world.tjs`'s `_envReset` and `quickmenu.tjs`'s `EventProxyLayer` both stop
//! a game's start-up otherwise.

use std::ffi::CString;
use std::path::PathBuf;

use bevy_krkr::script::{Tjs2Engine, last_error};
use bevy_krkr::sys;

fn data_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/game")
}

#[test]
fn scripts_foreach_and_get_object_keys() {
    let path = data_path();
    let c_path = CString::new(path.to_string_lossy().as_bytes()).expect("no NUL in the path");
    // SAFETY: called on the test thread, once for this process.
    let code = unsafe {
        sys::krkr_engine_set_screen_size(640, 480);
        sys::krkr_engine_init(c_path.as_ptr())
    };
    assert_eq!(code, sys::KRKR_OK, "krkr_engine_init: {}", last_error());
    let engine = Tjs2Engine::new().expect("Tjs2Engine::new");

    let check = |script: &str, expected: &str| {
        let value = engine
            .eval(script)
            .unwrap_or_else(|e| panic!("{script}\n{e}"));
        assert_eq!(value.to_string(), expected, "{script}");
    };

    // Every member is listed, however the object was built.
    check(
        r#"(function(){ var o = %["a"=>1, "b"=>2, "c"=>3]; return Scripts.getObjectKeys(o).count; })()"#,
        "3",
    );
    check(
        r#"(function(){ var o = %[]; o.x = 1; o.y = 2; return Scripts.getObjectKeys(o).count; })()"#,
        "2",
    );
    check(
        r#"(function(){ var o = new Dictionary(); o["k"] = 1; return Scripts.getObjectKeys(o).count; })()"#,
        "1",
    );
    // The shape KAG's quick menu uses: a dictionary that is a class member,
    // filled from a method, read back through the instance.
    engine
        .exec_script(
            r#"
                class Holder {
                    var names = %[];
                    function add(n) { names[n] = 1; }
                }
                global.h = new Holder();
                h.add("a");
                h.add("b");
            "#,
            "holder",
        )
        .expect("class with a member dictionary");
    check("Scripts.getObjectKeys(global.h.names).count", "2");

    // An object with no members gives an empty array. An array's keys are its
    // indices, which is also what `foreach` hands the callback.
    check("Scripts.getObjectKeys(%[]).count", "0");
    check("Scripts.getObjectKeys([1, 2, 3]).count", "3");
    check("Scripts.getObjectKeys([1, 2, 3])[2]", "2");

    // `f(keys*)` passes one argument per key: `quickmenu.tjs` relies on it to
    // report its own state (`this.dm(Scripts.getObjectKeys(owner.names)*)`), and
    // a call with no arguments at all is an error there.
    check(
        r#"(function(){ var o = %[]; o.n = 0; o.f = function(a, b) { this.n = (a !== void) + (b !== void); }; o.f(Scripts.getObjectKeys(%["a"=>1, "b"=>2])*); return o.n; })()"#,
        "2",
    );

    // scriptsEx passes key, value and extra arguments to the callback.
    check(
        r#"(function(){ var o = %[]; o.s = ""; Scripts.foreach([10, 20, 30], function(k, v, target) { target.s += v + ":" + k + " "; }, o); return o.s; })()"#,
        "10:0 20:1 30:2 ",
    );
    check(
        r#"(function(){ var o = %[]; o.n = 0; Scripts.foreach(%["a"=>1, "b"=>2, "c"=>3], function(k, v, target) { target.n += v; }, o); return o.n; })()"#,
        "6",
    );

    // A void extra argument stays void.
    check(
        r#"(function(){ global.__seen = 0; Scripts.foreach(%["a"=>1, "b"=>2], function(k, v, extra) { if(extra === void) global.__seen += 1; }, void); return global.__seen; })()"#,
        "2",
    );
    check(
        r#"(function(){ var visited = []; var result = Scripts.foreach([10,20,30], function(k,v,out){ out.add(k + ':' + v); if(k==1) return false; }, visited); return visited.join(',') + ':' + (result === false); })()"#,
        "0:10,1:20:1",
    );
    check(
        r#"(function(){ var state=%['seen'=>0]; var value=Scripts.foreach(%['a'=>1,'b'=>2], function(k,v,out){ out.seen++; return 'stop'; },state); return state.seen+':'+value; })()"#,
        "1:stop",
    );
    check(
        r#"(function(){ var options=%['first'=>%['menu'=>'one'],'second'=>%['menu'=>'two']]; var names=[]; Scripts.foreach(options,function(key,value,out){ out.add(key+':'+value.menu); },names); names.sort(); return names.join(','); })()"#,
        "first:one,second:two",
    );
    check(
        r#"(function(){ global.total=0; var receiver=%['total'=>0]; receiver.visit=function(k,v){ this.total+=v; }; Scripts.foreach([3,4],receiver.visit); return receiver.total+':'+global.total; })()"#,
        "0:7",
    );
    check(
        r#"(function(){ var original=%['layers'=>[%['name'=>'first','rect'=>[1,2,3,4]]],'value'=>42]; var copy=Scripts.clone(original); copy.layers[0].rect[0]=99; copy.layers[0].name='second'; return original.layers[0].rect[0]+':'+original.layers[0].name+':'+copy.layers[0].rect[0]+':'+copy.value; })()"#,
        "1:first:99:42",
    );
    check(
        "Scripts.clone(null)===null && Scripts.clone(void)===void && Scripts.clone('text')==='text'",
        "1",
    );
    engine
        .exec_script(
            "class CloneFallback { var number=5; } global.cloneFallback=new CloneFallback();",
            "clone fallback",
        )
        .unwrap();
    check("Scripts.clone(cloneFallback)===cloneFallback", "1");
    engine
        .exec_script("Scripts.dump();", "dump output")
        .unwrap();
    check(
        "Debug.getLastLog(100000).indexOf('TJS Context Dump')>=0",
        "1",
    );
}
