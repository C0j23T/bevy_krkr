use bevy_krkr::{script::Tjs2Engine, sys};
use std::ffi::CString;

fn chunk(name: &[u8], bytes: &[u8]) -> Vec<u8> {
    [name, &(bytes.len() as u64).to_le_bytes(), bytes].concat()
}

fn archive(name: &str, bytes: &[u8]) -> Vec<u8> {
    let size = bytes.len() as u64;
    let mut archive = b"XP3\r\n \n\x1a\x8b\x67\x01".to_vec();
    archive.extend_from_slice(&(19 + size).to_le_bytes());
    archive.extend_from_slice(bytes);
    let name: Vec<u8> = name.encode_utf16().flat_map(u16::to_le_bytes).collect();
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
    archive.push(0);
    archive.extend_from_slice(&(index.len() as u64).to_le_bytes());
    archive.extend(index);
    archive
}

#[test]
#[ignore = "requires tools/external-plugins/build.sh sqlite3"]
fn sqlite_bindings_storage_vfs_and_reload() {
    let project = std::env::temp_dir().join(format!("krkr-external-sqlite-{}", std::process::id()));
    std::fs::create_dir_all(project.join("plugin")).unwrap();
    let plugin = std::env::var("SQLITE_PLUGIN").unwrap_or_else(|_| {
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/target/external-plugins/sqlite3/libsqlite3.so"
        )
        .into()
    });
    std::fs::copy(plugin, project.join("plugin/libsqlite3.so")).unwrap();
    let path = CString::new(project.to_str().unwrap()).unwrap();
    assert_eq!(
        unsafe { sys::krkr_engine_init(path.as_ptr()) },
        sys::KRKR_OK
    );
    let engine = Tjs2Engine::new().unwrap();
    let run = |source: &str| engine.exec_script(source, "external sqlite3").unwrap();
    run(r#"
        Plugins.link("sqlite3.dll"); Plugins.link("sqlite3.dll");
        global.db = new Sqlite("database.db");
        if (!db.exec("create table records(id integer primary key, text_value text, real_value real, binary_value blob)")) throw db.errorMessage;
        if (!db.exec("insert into records values(?,?,?,?)", [1,"日本語🙂",1.25,<% 00, ff, 41 %>])) throw db.errorMessage;
        global.stmt = new SqliteStatement(db,"select id,text_value,real_value,binary_value,null as empty from records where id=:id");
        global.named = %[]; named[":id"] = 1;
        if (stmt.bind(named) != Sqlite.SQLITE_OK || !stmt.step()) throw "named binding failed";
    "#);
    assert_eq!(engine.eval("stmt.get('TEXT_VALUE')").unwrap(), "日本語🙂");
    assert_eq!(engine.eval("stmt.get(2)").unwrap(), "1.25");
    assert_eq!(
        engine
            .eval("stmt.getType(3)==SqliteStatement.SQLITE_BLOB")
            .unwrap(),
        "1"
    );
    assert_eq!(engine.eval("stmt.get(3)==<% 00, ff, 41 %>").unwrap(), "1");
    assert_eq!(engine.eval("stmt.isNull('empty')").unwrap(), "1");
    assert_eq!(
        engine.eval("stmt.get('empty', 'fallback')").unwrap(),
        "fallback"
    );
    assert_eq!(engine.eval("stmt.get().count").unwrap(), "5");
    assert_eq!(engine.eval("stmt.getName(1)").unwrap(), "text_value");
    assert_eq!(engine.eval("stmt.text_value").unwrap(), "日本語🙂");
    run(r#"
        stmt.close();
        if (stmt.open("insert into records(id,text_value) values(?,?)") != Sqlite.SQLITE_OK) throw "open failed";
        stmt.bindAt(2); stmt.bindAt("second");
        if (stmt.exec() != Sqlite.SQLITE_DONE) throw db.errorMessage;
        stmt.close();
        if (!db.begin()) throw "begin failed";
        db.exec("insert into records(id) values(3)");
        if (!db.rollback()) throw "rollback failed";
        global.rows=[];
        db.exec("select id,text_value from records order by id",[],function(id,text){ rows.push(id+":"+text); });
    "#);
    assert_eq!(
        engine.eval("rows.join('|')").unwrap(),
        "1:日本語🙂|2:second"
    );
    assert_eq!(
        engine
            .eval("db.execValue('select count(*) from records')")
            .unwrap(),
        "2"
    );
    assert_eq!(
        engine
            .eval("db.execValue(\"select cnt('日本語','本'), ncnt('Ａがﾞ','aｶ')\")")
            .unwrap(),
        "1"
    );
    assert_eq!(
        engine
            .eval("db.execValue(\"select ncnt('Ａがﾞ','aｶ')\")")
            .unwrap(),
        "1"
    );
    assert_eq!(engine.eval("db.exec('invalid sql')").unwrap(), "0");
    assert_ne!(engine.eval("db.errorCode").unwrap(), "0");
    run("invalidate stmt; stmt=void; invalidate db; db=void;");
    let database = std::fs::read(project.join("database.db")).unwrap();
    std::fs::write(
        project.join("archive.xp3"),
        archive("nested/database.db", &database),
    )
    .unwrap();
    run(r#"global.db = new Sqlite("archive.xp3>nested/database.db",true);"#);
    assert_eq!(
        engine
            .eval("db.execValue('select text_value from records where id=1')")
            .unwrap(),
        "日本語🙂"
    );
    assert_eq!(engine.eval("db.exec('delete from records')").unwrap(), "0");
    run("invalidate db; db=void;");
    assert_eq!(engine.eval("Plugins.unlink('sqlite3.dll')").unwrap(), "1");
    run("Plugins.link('sqlite3.dll'); global.db = new Sqlite(':memory:');");
    assert_eq!(engine.eval("db.execValue('select 42')").unwrap(), "42");
    run("invalidate db; db=void;");
    assert_eq!(engine.eval("Plugins.unlink('sqlite3.dll')").unwrap(), "1");
    drop(engine);
    unsafe { sys::krkr_engine_shutdown() };
    std::fs::remove_dir_all(project).unwrap();
}
