//! Generates `$OUT_DIR/bindings.rs` for `krkr_sys` and, in real mode, builds
//! and links the Kirikiroid2 engine core.
//!
//! Real mode (feature `real_ffi`, on by default) needs a Kirikiroid2 checkout
//! on the `bevy-port` branch: `$KIRIKIROID2_SRC`, or `Kirikiroid2/` in the
//! workspace root. Steps:
//!   1. krkr_codegen expands codegen/src/api into krkr_c_api.h + krkr_c_api.cpp
//!   2. bindgen turns krkr_c_api.h into bindings.rs
//!   3. cc compiles the engine core (codegen/src/port.rs) and the shim
//!   4. pkg-config links the system libraries the core uses
//!
//! Stub mode (no checkout, or `--no-default-features`): bindings.rs is
//! expanded from the same table; every function panics when called.

use std::env;
use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../codegen/src");
    println!("cargo:rerun-if-changed=../../codegen/templates");
    for var in ["KIRIKIROID2_SRC", "KRKR_CODEGEN_VERBOSE"] {
        println!("cargo:rerun-if-env-changed={var}");
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let explicit = env::var_os("KIRIKIROID2_SRC").map(PathBuf::from);
    let src = explicit
        .clone()
        .unwrap_or_else(|| manifest_dir.join("../../Kirikiroid2"));

    if !cfg!(feature = "real_ffi") {
        if explicit.is_some() {
            println!("cargo:warning=krkr_sys: KIRIKIROID2_SRC is set but feature `real_ffi` is off; using stubs");
        }
        return write_stubs(&out_dir);
    }
    if !src.is_dir() {
        if explicit.is_some() {
            panic!("krkr_sys: KIRIKIROID2_SRC={} is not a directory", src.display());
        }
        println!(
            "cargo:warning=krkr_sys: no Kirikiroid2 checkout at {} (set KIRIKIROID2_SRC); \
             building stub bindings that panic when called",
            src.display()
        );
        return write_stubs(&out_dir);
    }
    let src = src.canonicalize().expect("canonicalize Kirikiroid2 path");
    if !src.join(krkr_codegen::port::PORT_MARKER).is_file() {
        panic!(
            "krkr_sys: {} is a Kirikiroid2 checkout without the bevy port ({} is missing). \
             Check out its `bevy-port` branch.",
            src.display(),
            krkr_codegen::port::PORT_MARKER
        );
    }
    println!("cargo:rerun-if-changed={}", src.join("src").display());
    real::build(&src, &out_dir);
}

fn write_stubs(out_dir: &Path) {
    std::fs::write(out_dir.join("bindings.rs"), krkr_codegen::generate_rust_stubs())
        .expect("write stub bindings");
}

#[cfg(feature = "real_ffi")]
mod real {
    use krkr_codegen::port;
    use krkr_codegen::{run_codegen, CodegenConfig};
    use std::env;
    use std::fs;
    use std::path::{Path, PathBuf};

    pub fn build(src: &Path, out_dir: &Path) {
        // 1. codegen
        let header = out_dir.join("krkr_c_api.h");
        let shim = out_dir.join("krkr_c_api.cpp");
        run_codegen(&CodegenConfig {
            kirikiroid2_src: src.to_owned(),
            output_header: header.clone(),
            output_cpp: shim.clone(),
            verbose: env::var("KRKR_CODEGEN_VERBOSE").as_deref() == Ok("1"),
        })
        .expect("krkr_codegen failed");

        // 2. bindgen
        bindgen::Builder::default()
            .header(header.to_string_lossy())
            .allowlist_function("krkr_.*")
            .allowlist_type("krkr_.*|.*Opaque")
            .allowlist_var("KRKR_.*")
            .default_macro_constant_type(bindgen::MacroTypeVariation::Signed)
            .generate()
            .expect("bindgen failed")
            .write_to_file(out_dir.join("bindings.rs"))
            .expect("write bindings.rs");

        // 3. compile. Include paths of the system libraries first (no link
        // directives yet: they must come after the static libraries).
        let mut includes = port::include_dirs(src);
        includes.push(out_dir.to_owned());
        let storage_include = Path::new(env!("CARGO_MANIFEST_DIR")).join("../krkr_storage/include");
        println!("cargo:rerun-if-changed={}", storage_include.join("krkr_storage.h").display());
        includes.push(storage_include);
        includes.push(src.join("src/plugins/ncbind"));
        for (pkg, feature) in port::PKG_CONFIG {
            if port::enabled(*feature, feature_on) {
                includes.extend(probe(pkg, false).include_paths);
            }
        }

        let sources = port::collect_sources(src).expect("list Kirikiroid2 sources");
        // The directory-level `rerun-if-changed` in `main` only sees entries
        // being added or removed, not a file being edited, so every source and
        // header has to be listed or an edit to the engine silently reuses the
        // previous build.
        for path in sources.cpp.iter().chain(sources.c.iter()) {
            println!("cargo:rerun-if-changed={}", path.display());
        }
        for dir in port::include_dirs(src) {
            for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("h") {
                    println!("cargo:rerun-if-changed={}", path.display());
                }
            }
        }
        let configure = |b: &mut cc::Build| {
            b.includes(&includes).warnings(false).flag("-w").opt_level(2).debug(false);
            for f in port::FLAGS {
                b.flag(f);
            }
            for d in port::DEFINES {
                b.define(d, None);
            }
            for (d, feature) in port::FEATURE_DEFINES {
                if feature_on(feature) {
                    b.define(d, None);
                }
            }
        };
        let mut cpp = cc::Build::new();
        configure(&mut cpp);
        let plugins = write_plugins(src, out_dir);
        for header in ["ExternalPlugins.h", "ExternalTransitions.h"] {
            println!("cargo:rerun-if-changed={}", src.join("src/core/environ/bevy").join(header).display());
        }
        println!("cargo:rerun-if-changed={}", src.join("src/core/environ/bevy/plugins/ExtraTrans.h").display());
        for header in ["ExtransGeometry.h", "ExtransTurnTable.h"] {
            println!("cargo:rerun-if-changed={}", src.join("src/core/environ/bevy/plugins").join(header).display());
        }
        cpp.cpp(true).std("c++17").files(&sources.cpp).files(&plugins).file(&shim);
        println!("cargo:rerun-if-changed={}", src.join("src/core/environ/bevy/plugins/LayerExSaveUtils.inc").display());
        cpp.compile("krkr_core");
        let mut c = cc::Build::new();
        configure(&mut c);
        c.std("gnu11").files(&sources.c);
        c.compile("krkr_core_c");

        // 4. link the system libraries.
        for (pkg, feature) in port::PKG_CONFIG {
            if port::enabled(*feature, feature_on) {
                probe(pkg, true);
            }
        }

    }

    fn write_plugins(src: &Path, out_dir: &Path) -> Vec<PathBuf> {
        let enabled: Vec<&port::Plugin> = port::PLUGINS
            .iter()
            .filter(|plugin| port::enabled(plugin.feature, feature_on))
            .collect();

        let mut header = String::from(
            "// Generated by krkr_sys/build.rs from `PLUGINS` in codegen/src/port.rs.\n\
             #pragma once\n\n\
             /// Defines the keep-alive symbol of a plugin's wrapper translation unit.\n\
             #define KRKR_PLUGIN(stem) void TVPBevyPlugin_##stem() {}\n\n",
        );
        header += "/// Every plugin in this build, as `X(stem, module)`.\n#define KRKR_PLUGINS(X) ";
        header += &list_lines(&enabled, |p| format!("X({}, \"{}\")", stem(p), p.module));
        header += "\n/// The modules the loader attaches at start-up, as `X(module)`.\n";
        header += "#define KRKR_INTERNAL_PLUGINS(X) ";
        header += &list_lines(
            &enabled.iter().copied().filter(|p| p.internal).collect::<Vec<_>>(),
            |p| format!("X(\"{}\")", p.module),
        );
        header += "\n#define KRKR_PLUGIN_DECLARE(stem, module) void TVPBevyPlugin_##stem();\n";
        header += "KRKR_PLUGINS(KRKR_PLUGIN_DECLARE)\n#undef KRKR_PLUGIN_DECLARE\n";
        fs::write(out_dir.join("krkr_plugins.h"), header).expect("write krkr_plugins.h");

        let mut wrappers = Vec::new();
        for plugin in &enabled {
            let wrapper = out_dir.join(format!("krkr_plugin_{}.cpp", stem(plugin)));
            let source = src.join(plugin.source);
            let body = format!(
                "// Generated by krkr_sys/build.rs; see write_plugins there.\n\
                 #include \"krkr_plugins.h\"\n\
                 #include \"{}\"\n\
                 KRKR_PLUGIN({})\n",
                source.display(),
                stem(plugin),
            );
            fs::write(&wrapper, body).expect("write plugin wrapper");
            wrappers.push(wrapper);
        }
        wrappers
    }

    /// `\` then one `X(...)` per line, ` \` between them: a macro list that
    /// reads as one logical line to the preprocessor.
    fn list_lines<T>(items: &[T], mut item: impl FnMut(&T) -> String) -> String {
        if items.is_empty() {
            return String::new();
        }
        let mut out = String::from("\\");
        for (i, it) in items.iter().enumerate() {
            out += "\n    ";
            out += &item(it);
            if i + 1 < items.len() {
                out += " \\";
            }
        }
        out
    }

    /// File stem of a plugin source, which names its keep-alive symbol.
    fn stem(plugin: &port::Plugin) -> String {
        Path::new(plugin.source)
            .file_stem()
            .and_then(|s| s.to_str())
            .expect("plugin source has a file name")
            .to_owned()
    }

    /// Whether a `krkr_sys` feature is on, as cargo reports it to build scripts.
    fn feature_on(name: &str) -> bool {
        let var = format!("CARGO_FEATURE_{}", name.to_uppercase().replace('-', "_"));
        println!("cargo:rerun-if-env-changed={var}");
        env::var_os(var).is_some()
    }

    fn probe(pkg: &str, link: bool) -> pkg_config::Library {
        pkg_config::Config::new()
            .cargo_metadata(link)
            .probe(pkg)
            .unwrap_or_else(|e| panic!("krkr_sys: system library `{pkg}` not found via pkg-config: {e}"))
    }


}

#[cfg(not(feature = "real_ffi"))]
mod real {
    pub fn build(_: &std::path::Path, out_dir: &std::path::Path) {
        super::write_stubs(out_dir);
    }
}
