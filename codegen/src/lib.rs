//! `krkr_codegen` – binding generator for bevy_krkr.
//!
//! Pipeline (modelled on bevy_mod_scripting's generate → collect → expand):
//!
//! 1. **generate**: the C ABI surface is declared once, in [`api`]: one
//!    [`FnDecl`] per function, with its C and Rust signature and the C++
//!    statements that forward it to Kirikiroid2.
//! 2. **collect**: copyright notices are collected from the Kirikiroid2
//!    sources the shim is compiled against, so they are re-emitted verbatim.
//! 3. **expand**: the tables are expanded into
//!    - `krkr_c_api.h`   – `extern "C"` header fed to bindgen,
//!    - `krkr_c_api.cpp` – the shim: `templates/shim_prelude.cpp` followed by
//!      every function body wrapped in an exception guard,
//!    - Rust stub bindings (used when no Kirikiroid2 checkout is available).
//!
//! [`port`] lists the Kirikiroid2 sources the shim is linked with.

pub mod api;
pub mod port;

use anyhow::{Context, Result};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

pub use api::{
    functions, CallbackType, FnDecl, KirikiridModule, Param, CALLBACK_TYPES, CONSTANTS, GROUPS,
    OPAQUE_TYPES,
};

/// Support code expanded at the top of the shim.
pub const SHIM_PRELUDE: &str = include_str!("../templates/shim_prelude.cpp");

/// Configuration passed to [`run_codegen`].
#[derive(Debug, Clone)]
pub struct CodegenConfig {
    /// Root of a Kirikiroid2 source checkout.
    pub kirikiroid2_src: PathBuf,
    /// Where to write `krkr_c_api.h`.
    pub output_header: PathBuf,
    /// Where to write `krkr_c_api.cpp`.
    pub output_cpp: PathBuf,
    /// Print progress to stderr.
    pub verbose: bool,
}

/// Directories (relative to the Kirikiroid2 root) whose copyright notices are
/// carried into the generated files: the code the shim calls into.
pub const SCAN_DIRS: &[&str] = &[
    "src/core/tjs2",
    "src/core/base",
    "src/core/utils",
    "src/core/visual",
    "src/core/environ/bevy",
    "src/plugins/ncbind",
];

/// Run the full pipeline: write the C header and the C++ shim.
pub fn run_codegen(cfg: &CodegenConfig) -> Result<()> {
    for out in [&cfg.output_header, &cfg.output_cpp] {
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create dir {}", parent.display()))?;
        }
    }
    let dirs: Vec<PathBuf> = SCAN_DIRS
        .iter()
        .map(|d| cfg.kirikiroid2_src.join(d))
        .collect();
    let notices = collect_copyright_notices(&dirs, cfg.verbose);
    log(cfg, format!("collected {} copyright notice(s)", notices.len()));

    fs::write(&cfg.output_header, generate_c_header(&notices))
        .with_context(|| format!("write {}", cfg.output_header.display()))?;
    fs::write(&cfg.output_cpp, generate_cpp_shim(&notices))
        .with_context(|| format!("write {}", cfg.output_cpp.display()))?;
    log(
        cfg,
        format!(
            "wrote {} and {}",
            cfg.output_header.display(),
            cfg.output_cpp.display()
        ),
    );
    Ok(())
}

fn log(cfg: &CodegenConfig, msg: String) {
    if cfg.verbose {
        eprintln!("[krkr_codegen] {msg}");
    }
}

// ─── collect ────────────────────────────────────────────────────────────────

/// Extract the first copyright comment block of every source file under
/// `dirs`, de-duplicated by first line. Files are read as bytes: many
/// Kirikiroid2 sources are Shift_JIS, but their notices are ASCII.
pub fn collect_copyright_notices(dirs: &[PathBuf], verbose: bool) -> Vec<String> {
    let mut notices: Vec<String> = Vec::new();
    for dir in dirs.iter().filter(|d| d.exists()) {
        let mut files: Vec<PathBuf> = walkdir::WalkDir::new(dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file() && is_source(e.path()))
            .map(|e| e.into_path())
            .collect();
        files.sort();
        for path in files {
            let Ok(bytes) = fs::read(&path) else {
                continue;
            };
            let text: String = String::from_utf8_lossy(&bytes)
                .chars()
                .map(|c| if c.is_ascii() { c } else { '?' })
                .collect();
            let Some(notice) = extract_copyright_block(&text) else {
                continue;
            };
            let key = notice.lines().next().unwrap_or_default().trim().to_owned();
            let seen = notices
                .iter()
                .any(|n| n.lines().next().unwrap_or_default().trim() == key);
            if !seen {
                if verbose {
                    eprintln!("[krkr_codegen] copyright notice from {}", path.display());
                }
                notices.push(notice);
            }
        }
    }
    notices
}

fn is_source(p: &Path) -> bool {
    matches!(
        p.extension().and_then(|s| s.to_str()),
        Some("h" | "hpp" | "c" | "cpp")
    )
}

/// Return the leading comment region of a source file (every comment before
/// the first line of code) if it mentions "copyright".
pub fn extract_copyright_block(src: &str) -> Option<String> {
    let src = src.trim_start_matches('\u{feff}');
    let mut region = Vec::new();
    let mut in_block = false;
    for line in src.lines() {
        let t = line.trim();
        if in_block {
            region.push(line);
            in_block = !t.contains("*/");
        } else if t.is_empty() || t.starts_with("//") {
            region.push(line);
        } else if t.starts_with("/*") {
            region.push(line);
            in_block = !t.strip_prefix("/*").is_some_and(|rest| rest.contains("*/"));
        } else {
            break;
        }
    }
    let block = region.join("\n").trim().to_owned();
    block.to_lowercase().contains("copyright").then_some(block)
}

fn notices_comment(notices: &[String]) -> String {
    let mut out = String::new();
    if notices.is_empty() {
        return out;
    }
    out.push_str("/*\n * Copyright notices collected from the Kirikiroid2 sources.\n");
    out.push_str(" * DO NOT REMOVE (see LICENSE-THIRD-PARTY.md).\n *\n");
    for n in notices {
        for line in n.lines() {
            let line = line.trim();
            if line.chars().all(|c| matches!(c, '/' | '-' | '*' | '=')) {
                continue; // separator rules
            }
            let line = line
                .trim_start_matches("/*")
                .trim_end_matches("*/")
                .trim_start_matches("//")
                .trim()
                .trim_start_matches('*')
                .trim();
            let _ = writeln!(out, " * {}", line.replace("*/", "* /"));
        }
        out.push_str(" *\n");
    }
    out.push_str(" */\n\n");
    out
}

// ─── expand ─────────────────────────────────────────────────────────────────

const GENERATED_BANNER: &str =
    "AUTO-GENERATED by krkr_codegen from codegen/src/api. Do not edit.";

fn c_signature(f: &FnDecl) -> String {
    let params = if f.params.is_empty() {
        "void".to_owned()
    } else {
        f.params
            .iter()
            .map(|p| format!("{} {}", p.c_ty, p.name))
            .collect::<Vec<_>>()
            .join(", ")
    };
    format!("{} {}({})", f.c_ret, f.name, params)
}

/// Expand the tables into the C header handed to bindgen.
pub fn generate_c_header(notices: &[String]) -> String {
    let mut out = notices_comment(notices);
    let _ = writeln!(
        out,
        "/* krkr_c_api.h – C ABI over Kirikiroid2.\n * {GENERATED_BANNER}\n */"
    );
    out.push_str("#ifndef KRKR_C_API_H\n#define KRKR_C_API_H\n\n");
    out.push_str("#include <stddef.h>\n#include <stdint.h>\n\n");
    out.push_str("#ifdef __cplusplus\nextern \"C\" {\n#endif\n\n");
    out.push_str("typedef int krkr_result_t;\n");
    for (name, value, doc) in CONSTANTS {
        let _ = writeln!(out, "/** {doc} */\n#define {name} {value}");
    }
    out.push('\n');
    for (name, doc) in OPAQUE_TYPES {
        let _ = writeln!(out, "/** {doc} */\ntypedef struct {name} {name};");
    }
    for cb in CALLBACK_TYPES {
        let _ = writeln!(out, "\n/** {} */\ntypedef void (*{})({});", cb.doc, cb.name, cb.c_params);
    }
    for group in GROUPS {
        let _ = writeln!(out, "\n/* ── {} ── */", group[0].module.title());
        for f in group.iter() {
            let _ = writeln!(out, "/** {} */\n{};", f.doc, c_signature(f));
        }
    }
    out.push_str("\n#ifdef __cplusplus\n} /* extern \"C\" */\n#endif\n\n#endif /* KRKR_C_API_H */\n");
    out
}

/// Expand the tables into the C++ shim: the prelude, then one guarded
/// definition per function.
pub fn generate_cpp_shim(notices: &[String]) -> String {
    let mut out = notices_comment(notices);
    let _ = writeln!(
        out,
        "// krkr_c_api.cpp – extern \"C\" shim over Kirikiroid2.\n// {GENERATED_BANNER}\n"
    );
    out.push_str("#include \"krkr_c_api.h\"\n\n");
    out.push_str(SHIM_PRELUDE);
    for group in GROUPS {
        let _ = writeln!(out, "\n// ── {} ──", group[0].module.title());
        for f in group.iter() {
            let ret = f.failure_value().unwrap_or("");
            let _ = writeln!(
                out,
                "\n// forwards to: {}\n{} {{\n    try {{{}\n    }} KRKR_SHIM_CATCH({})\n}}",
                f.cpp_target,
                c_signature(f),
                f.cpp_body.trim_end().replace("\n    ", "\n        "),
                ret
            );
        }
    }
    out
}

/// Expand the tables into Rust stub bindings with the same signatures as
/// bindgen would emit. Each stub panics with a diagnostic when called.
pub fn generate_rust_stubs() -> String {
    let mut out = String::new();
    let _ = writeln!(out, "// Stub bindings for krkr_sys. {GENERATED_BANNER}");
    out.push_str(
        "// Built because no Kirikiroid2 checkout was used (see krkr_sys/build.rs).\n\
         // Signatures match bindgen output; every function panics when called.\n\n\
         #[allow(unused_imports)]\nuse ::std::os::raw::{c_char, c_int, c_uint, c_void};\n\n\
         pub type krkr_result_t = c_int;\n",
    );
    for (name, value, doc) in CONSTANTS {
        let _ = writeln!(out, "/// {doc}\npub const {name}: i32 = {value};");
    }
    for (name, doc) in OPAQUE_TYPES {
        let _ = writeln!(out, "/// {doc}\n#[repr(C)]\npub struct {name} {{\n    _unused: [u8; 0],\n}}");
    }
    for cb in CALLBACK_TYPES {
        let _ = writeln!(
            out,
            "/// {}\npub type {} = ::std::option::Option<unsafe extern \"C\" fn({})>;",
            cb.doc, cb.name, cb.rust_params
        );
    }
    for f in functions() {
        let params = f
            .params
            .iter()
            .map(|p| format!("_{}: {}", p.name, p.rust_ty))
            .collect::<Vec<_>>()
            .join(", ");
        let ret = if f.rust_ret.is_empty() {
            String::new()
        } else {
            format!(" -> {}", f.rust_ret)
        };
        let _ = writeln!(
            out,
            "\n/// {doc}\npub unsafe fn {name}({params}){ret} {{\n    panic!(\"bevy_krkr: `{name}` is a stub: \
             krkr_sys was built without a Kirikiroid2 checkout (see krkr_sys/build.rs)\")\n}}",
            doc = f.doc,
            name = f.name,
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_function_is_in_every_artefact() {
        let h = generate_c_header(&[]);
        let shim = generate_cpp_shim(&[]);
        let stubs = generate_rust_stubs();
        for f in functions() {
            assert!(h.contains(&format!(" {}(", f.name)), "{} missing in header", f.name);
            assert!(shim.contains(&format!(" {}(", f.name)), "{} missing in shim", f.name);
            assert!(stubs.contains(&format!("pub unsafe fn {}(", f.name)), "{} missing in stubs", f.name);
        }
    }

    #[test]
    fn names_are_unique() {
        let mut names: Vec<_> = functions().map(|f| f.name).collect();
        names.sort();
        let n = names.len();
        names.dedup();
        assert_eq!(n, names.len());
    }

    /// The stubs must have the signatures bindgen emits, or the crate would
    /// compile in one mode and not the other.
    #[test]
    fn rust_types_follow_bindgen_mapping() {
        fn expected(c: &str) -> Option<&str> {
            Some(match c {
                "const char*" => "*const c_char",
                "char**" => "*mut *mut c_char",
                "const char* const*" => "*const *const c_char",
                "void*" => "*mut c_void",
                "int" => "c_int",
                "unsigned int" => "c_uint",
                "size_t" => "usize",
                "size_t*" => "*mut usize",
                "uint64_t" => "u64",
                "uint8_t**" => "*mut *mut u8",
                "const uint8_t*" => "*const u8",
                "krkr_result_t" => "krkr_result_t",
                "void" => "",
                c if c.ends_with("Opaque*") => return None,
                c if c.ends_with("_callback_t") => c,
                _ => return None,
            })
        }
        for f in functions() {
            for p in f.params {
                if let Some(rust) = expected(p.c_ty) {
                    assert_eq!(rust, p.rust_ty, "{}::{}", f.name, p.name);
                }
            }
            if let Some(rust) = expected(f.c_ret) {
                assert_eq!(rust, f.rust_ret, "{} return", f.name);
            }
        }
    }

    #[test]
    fn copyright_block_after_separator_is_extracted() {
        let src = "//------\n/*\n\tTJS2 Script Engine\n\tCopyright (C) 2000 W.Dee\n*/\n//----\n#include <x>\n";
        let block = extract_copyright_block(src).unwrap();
        assert!(block.contains("W.Dee"));
        assert!(notices_comment(&[block]).contains("Copyright (C) 2000 W.Dee"));
        assert!(extract_copyright_block("int x;").is_none());
        assert!(extract_copyright_block("// hello\nint x;").is_none());
    }

    #[test]
    fn shim_bodies_are_guarded() {
        let shim = generate_cpp_shim(&[]);
        assert_eq!(shim.matches("} KRKR_SHIM_CATCH(").count(), functions().count());
        assert!(shim.contains("krkr_layer_get_pixels(KrkrLayerOpaque* layer) {\n    try {"));
    }
}
