use bevy_krkr::{script::Tjs2Engine, sys};
use std::ffi::CString;

fn array(values: &[u32]) -> Vec<u8> {
    let mut bytes = vec![0x10];
    bytes.extend((values.len() as u32).to_le_bytes());
    bytes.push(16);
    bytes.extend(values.iter().flat_map(|v| v.to_le_bytes()));
    bytes
}

fn psb(version: u16, title: &str) -> Vec<u8> {
    let mut bytes = vec![0; if version == 3 { 44 } else { 40 }];
    bytes[..4].copy_from_slice(b"PSB\0");
    bytes[4..6].copy_from_slice(&version.to_le_bytes());
    let names_at = bytes.len() as u32;
    if version == 1 {
        bytes.extend(b"title\0blob\0");
    } else {
        let mut nodes = vec![0; 256];
        let mut bases = vec![0; 256];
        bases[0] = 1;
        let mut next = 256;
        let mut leaves = Vec::new();
        for word in [b"title".as_slice(), b"blob"] {
            let mut parent = 0;
            for &ch in word {
                if bases[parent] == 0 {
                    bases[parent] = next as u32;
                    next += 256;
                }
                let node = bases[parent] as usize + ch as usize;
                nodes.resize(nodes.len().max(node + 1), 0);
                bases.resize(nodes.len(), 0);
                nodes[node] = parent as u32;
                parent = node;
            }
            leaves.push(parent as u32);
        }
        let slots = [nodes.len() as u32, nodes.len() as u32 + 1];
        nodes.extend(leaves);
        bases.resize(nodes.len(), 0);
        bytes.extend(array(&bases));
        bytes.extend(array(&nodes));
        bytes.extend(array(&slots));
    }
    let v1_indexes = bytes.len() as u32;
    if version == 1 {
        bytes.extend(array(&[0, 6]));
    }
    let strings_at = bytes.len() as u32;
    bytes.extend(array(&[0]));
    let strings_data = bytes.len() as u32;
    bytes.extend(title.as_bytes());
    bytes.push(0);
    let chunk_offsets = bytes.len() as u32;
    bytes.extend(array(&[0]));
    let resource: Vec<u8> = [0xff, 0xfe]
        .into_iter()
        .chain("'resource';".encode_utf16().flat_map(u16::to_le_bytes))
        .collect();
    let chunk_lengths = bytes.len() as u32;
    bytes.extend(array(&[resource.len() as u32]));
    let chunk_data = bytes.len() as u32;
    bytes.extend(resource);
    let entries = bytes.len() as u32;
    bytes.push(0x21);
    if version == 1 {
        bytes.extend(array(&[0, 6]));
        bytes.extend(0u32.to_le_bytes());
        bytes.extend([0x15, 0]);
        bytes.extend(1u32.to_le_bytes());
        bytes.extend([0x19, 0]);
    } else {
        bytes.extend(array(&[0, 1]));
        bytes.extend(array(&[0, 2]));
        bytes.extend([0x15, 0, 0x19, 0]);
    }
    for (i, value) in [
        if version == 1 { v1_indexes } else { names_at },
        names_at,
        strings_at,
        strings_data,
        chunk_offsets,
        chunk_lengths,
        chunk_data,
        entries,
    ]
    .into_iter()
    .enumerate()
    {
        bytes[8 + i * 4..12 + i * 4].copy_from_slice(&value.to_le_bytes());
    }
    bytes
}

#[test]
#[ignore = "requires tools/external-plugins/build.sh psbfile"]
fn psb_versions_mdf_resources_and_reload() {
    let project = std::env::temp_dir().join(format!("krkr-external-psb-{}", std::process::id()));
    std::fs::create_dir_all(project.join("plugin")).unwrap();
    std::fs::copy(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/target/external-plugins/psbfile/libpsbfile.so"
        ),
        project.join("plugin/libpsbfile.so"),
    )
    .unwrap();
    for version in 1..=3 {
        std::fs::write(
            project.join(format!("v{version}.psb")),
            psb(version, "日本🙂"),
        )
        .unwrap();
    }
    let plain = psb(3, "MDF");
    let script = "import sys,zlib;d=sys.stdin.buffer.read();sys.stdout.buffer.write(b'mdf\\0'+len(d).to_bytes(4,'little')+zlib.compress(d))";
    let mut compressor = std::process::Command::new("python3")
        .args(["-c", script])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    std::io::Write::write_all(compressor.stdin.take().as_mut().unwrap(), &plain).unwrap();
    let compressed = compressor.wait_with_output().unwrap();
    assert!(compressed.status.success());
    std::fs::write(project.join("packed.psb"), compressed.stdout).unwrap();
    std::fs::write(project.join("bad.psb"), b"PSB\0bad").unwrap();
    let mut flagged = psb(2, "flagged");
    flagged[6..8].copy_from_slice(&1u16.to_le_bytes());
    std::fs::write(project.join("flagged.psb"), flagged).unwrap();
    std::fs::write(project.join("size.psb"), b"mdf\0\xff\xff\xff\xffbad").unwrap();

    let mut invalid = psb(3, "bad");
    invalid[36..40].copy_from_slice(&u32::MAX.to_le_bytes());
    std::fs::write(project.join("offset.psb"), invalid).unwrap();
    let real_sample = std::env::var("PSB_SAMPLE").ok();
    if let Some(ref sample) = real_sample {
        std::fs::copy(sample, project.join("real.pimg")).unwrap();
    }
    let path = CString::new(project.to_str().unwrap()).unwrap();
    assert_eq!(
        unsafe { sys::krkr_engine_init(path.as_ptr()) },
        sys::KRKR_OK
    );
    let engine = Tjs2Engine::new().unwrap();
    let run = |s: &str| engine.exec_script(s, "external PSB").unwrap();
    run("Plugins.link('psbfile.dll'); global.p=new PSBFile();");
    run("global.constructorProbe=new PSBFile('v3.psb');");
    assert_eq!(
        engine.eval("constructorProbe.root.title").unwrap(),
        "日本🙂"
    );
    run("invalidate constructorProbe; constructorProbe=void;");
    let octet = psb(2, "octet")
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join(", ");
    run(&format!(
        "global.bytes=<% {octet} %>; global.constructorProbe=new PSBFile(bytes);"
    ));
    assert_eq!(engine.eval("constructorProbe.root.title").unwrap(), "octet");
    assert_eq!(engine.eval("p.load(bytes)").unwrap(), "1");
    assert_eq!(engine.eval("p.root.title").unwrap(), "octet");
    run("invalidate constructorProbe; constructorProbe=void; bytes=void;");

    for version in 1..=3 {
        assert_eq!(
            engine.eval(&format!("p.load('v{version}.psb')")).unwrap(),
            "1"
        );
        assert_eq!(engine.eval("p.root.title").unwrap(), "日本🙂");
        assert_eq!(
            engine
                .eval(&format!(
                    "Storages.isExistentStorage('psb://v{version}.psb/blob')"
                ))
                .unwrap(),
            "1"
        );
        assert_eq!(
            engine
                .eval(&format!("Scripts.evalStorage('psb://v{version}.psb/blob')"))
                .unwrap(),
            "resource"
        );
    }
    assert_eq!(engine.eval("p.load('packed.psb')").unwrap(), "1");
    assert_eq!(engine.eval("p.root.title").unwrap(), "MDF");
    assert_eq!(
        engine
            .eval("Scripts.evalStorage('psb://packed.psb/blob')")
            .unwrap(),
        "resource"
    );
    assert_eq!(
        engine
            .eval("Scripts.evalStorage('psb://packed.psb/blob')")
            .unwrap(),
        "resource"
    );
    std::fs::write(project.join("packed.psb"), psb(2, "replacement")).unwrap();
    assert_eq!(engine.eval("p.load('packed.psb')").unwrap(), "1");
    assert_eq!(engine.eval("p.root.title").unwrap(), "replacement");
    if real_sample.is_some() {
        assert_eq!(engine.eval("p.load('real.pimg')").unwrap(), "1");
        assert_eq!(engine.eval("typeof p.root").unwrap(), "Object");
    }
    assert_eq!(engine.eval("p.load('flagged.psb')").unwrap(), "1");
    assert_eq!(engine.eval("p.root.title").unwrap(), "flagged");
    for file in ["bad.psb", "offset.psb", "size.psb"] {
        assert_eq!(engine.eval(&format!("p.load('{file}')")).unwrap(), "0");
        assert_eq!(engine.eval("typeof p.root").unwrap(), "void");
    }
    assert_eq!(engine.eval("p.load('v2.psb')").unwrap(), "1");
    assert_eq!(engine.eval("p.root.title").unwrap(), "日本🙂");
    assert_eq!(engine.eval("Plugins.unlink('psbfile.dll')").unwrap(), "0");
    run("invalidate p; p=void;");
    assert_eq!(engine.eval("Plugins.unlink('psbfile.dll')").unwrap(), "1");
    drop(engine);
    unsafe { sys::krkr_engine_shutdown() };
    std::fs::remove_dir_all(project).unwrap();
}
