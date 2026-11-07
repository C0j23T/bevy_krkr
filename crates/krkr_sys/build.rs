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
    use std::path::Path;

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
        for pkg in port::PKG_CONFIG {
            includes.extend(probe(pkg, false).include_paths);
        }
        let opencv = probe_opencv(false);
        includes.extend(opencv.0.include_paths.iter().cloned());

        let sources = port::collect_sources(src).expect("list Kirikiroid2 sources");
        let configure = |b: &mut cc::Build| {
            b.includes(&includes).warnings(false).flag("-w").opt_level(2).debug(false);
            for f in port::FLAGS {
                b.flag(f);
            }
            for d in port::DEFINES {
                b.define(d, None);
            }
        };
        let mut cpp = cc::Build::new();
        configure(&mut cpp);
        cpp.cpp(true).std("c++17").files(&sources.cpp).file(&shim);
        cpp.compile("krkr_core");
        let mut c = cc::Build::new();
        configure(&mut c);
        c.std("gnu11").files(&sources.c);
        c.compile("krkr_core_c");

        // 4. link the system libraries.
        for pkg in port::PKG_CONFIG {
            probe(pkg, true);
        }
        let (lib, libs) = opencv;
        for dir in &lib.link_paths {
            println!("cargo:rustc-link-search=native={}", dir.display());
        }
        for name in libs {
            println!("cargo:rustc-link-lib={name}");
        }
    }

    fn probe(pkg: &str, link: bool) -> pkg_config::Library {
        pkg_config::Config::new()
            .cargo_metadata(link)
            .probe(pkg)
            .unwrap_or_else(|e| panic!("krkr_sys: system library `{pkg}` not found via pkg-config: {e}"))
    }

    fn probe_opencv(link: bool) -> (pkg_config::Library, &'static [&'static str]) {
        for (pkg, libs) in port::OPENCV {
            if let Ok(lib) = pkg_config::Config::new().cargo_metadata(link).probe(pkg) {
                return (lib, libs);
            }
        }
        let names: Vec<_> = port::OPENCV.iter().map(|(p, _)| *p).collect();
        panic!("krkr_sys: OpenCV not found via pkg-config (tried {names:?})");
    }
}

#[cfg(not(feature = "real_ffi"))]
mod real {
    pub fn build(_: &std::path::Path, out_dir: &std::path::Path) {
        super::write_stubs(out_dir);
    }
}
