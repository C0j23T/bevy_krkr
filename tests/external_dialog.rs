use bevy_krkr::{script::Tjs2Engine, sys};
use std::ffi::CString;

#[test]
#[ignore = "requires tools/external-plugins/build.sh win32dialog"]
fn dialog_control_model_and_host_protocol() {
    let project = std::env::temp_dir().join(format!("krkr-external-dialog-{}", std::process::id()));
    std::fs::create_dir_all(project.join("plugin")).unwrap();
    std::fs::copy(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/target/external-plugins/win32dialog/libwin32dialog.so"
        ),
        project.join("plugin/libwin32dialog.so"),
    )
    .unwrap();
    let path = CString::new(project.to_str().unwrap()).unwrap();
    assert_eq!(
        unsafe { sys::krkr_engine_init(path.as_ptr()) },
        sys::KRKR_OK
    );
    let engine = Tjs2Engine::new().unwrap();
    let run = |s: &str| engine.exec_script(s, "external dialog").unwrap();
    run(r#"
        Plugins.link('win32dialog.dll');
        global.d=new WIN32Dialog(null);
        global.header=new WIN32Dialog.Header();
        header.store(%[title:'日本語',cx:240,cy:120]);
        global.edit=new WIN32Dialog.Items();
        edit.store(%[id:10,windowClass:WIN32Dialog.EDIT,title:'初期',x:4,y:8,cx:100,cy:20]);
        global.check=new WIN32Dialog.Items();
        check.store(%[id:11,windowClass:WIN32Dialog.BUTTON,title:'確認',style:3]);
        global.list=new WIN32Dialog.Items();
        list.store(%[id:12,windowClass:WIN32Dialog.LISTBOX,title:'選択']);
        global.accept=new WIN32Dialog.Items(); accept.store(%[id:1,windowClass:WIN32Dialog.BUTTON,title:'決定']);
        global.cancel=new WIN32Dialog.Items(); cancel.store(%[id:2,windowClass:WIN32Dialog.BUTTON,title:'取消']);
        d.makeTemplate(header,edit,check,list,accept,cancel);
        d.setItemText(10,'変更🙂');
        d.setItemEnabled(10,false);
        d.setItemPos(10,12,13); d.setItemSize(10,80,24);
        d.sendItemMessage(11,WIN32Dialog.BM_SETCHECK,1);
        d.sendItemMessage(12,WIN32Dialog.LB_ADDSTRING,0,'甲');
        d.sendItemMessage(12,WIN32Dialog.LB_ADDSTRING,0,'乙');
        d.sendItemMessage(12,WIN32Dialog.LB_SETCURSEL,1);
    "#);
    for (expression, expected) in [
        ("WIN32Dialog.BM_SETCHECK", "241"),
        ("WIN32Dialog.BM_GETCHECK", "240"),
        ("d.isExistentItem(11)", "1"),
        ("d.getItemText(10)", "変更🙂"),
        ("d.getItemEnabled(10)", "0"),
        ("d.getItemLeft(10)", "12"),
        ("d.getItemWidth(10)", "80"),
        ("d.sendItemMessage(11,WIN32Dialog.BM_GETCHECK)", "1"),
        ("d.sendItemMessage(12,WIN32Dialog.LB_GETCOUNT)", "2"),
        ("d.sendItemMessage(12,WIN32Dialog.LB_GETCURSEL)", "1"),
        ("d.isExistentItem(10)", "1"),
        ("d.isExistentItem(99)", "0"),
    ] {
        assert_eq!(engine.eval(expression).unwrap(), expected, "{expression}");
    }
    run(r#"
        global.originalInform=System.inform; global.originalInput=System.inputString;
        global.choices=[]; global.prompts=[];
        System.inform=function(text,caption,buttons){choices.add(%[text:text,caption:caption,buttons:buttons]); return 1;};
        System.inputString=function(caption,prompt,value){prompts.add(%[caption:caption,prompt:prompt,value:value]); return '入力🙂';};
        global.messageResult=WIN32Dialog.messageBox(0,'本文🙂','題名',WIN32Dialog.MB_OKCANCEL);
        d.setItemEnabled(10,true);
        d.open();
        System.inform=originalInform; System.inputString=originalInput;
    "#);
    assert_eq!(engine.eval("messageResult").unwrap(), "2");
    assert_eq!(engine.eval("choices[0].caption").unwrap(), "題名");
    assert_eq!(engine.eval("choices[0].text").unwrap(), "本文🙂");
    assert_eq!(engine.eval("choices[0].buttons.count").unwrap(), "2");
    assert_eq!(engine.eval("d.getItemText(10)").unwrap(), "入力🙂");
    assert_eq!(engine.eval("prompts[0].value").unwrap(), "変更🙂");
    run(
        "invalidate d; d=void; invalidate header; header=void; invalidate edit;edit=void;invalidate check;check=void;invalidate list;list=void;invalidate accept;accept=void;invalidate cancel;cancel=void;",
    );
    assert_eq!(
        engine.eval("Plugins.unlink('win32dialog.dll')").unwrap(),
        "1"
    );
    run("global.removed=false;try{WIN32Dialog;}catch(e){removed=true;}");
    assert_eq!(engine.eval("removed").unwrap(), "1");
    drop(engine);
    unsafe { sys::krkr_engine_shutdown() };
    std::fs::remove_dir_all(project).unwrap();
}
