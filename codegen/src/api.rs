//! The C ABI surface over Kirikiroid2, declared once.
//!
//! Every generated artefact is expanded from these tables:
//! - `krkr_c_api.h`   – the `extern "C"` header handed to bindgen,
//! - `krkr_c_api.cpp` – the shim: each [`FnDecl::cpp_body`] wrapped in an
//!   exception guard, after the shared support code in
//!   `templates/shim_prelude.cpp`,
//! - Rust stub bindings with the same signatures, used when no Kirikiroid2
//!   checkout is available.
//!
//! To expose a new Kirikiroid2 capability, add one [`FnDecl`] to the module of
//! its subsystem (see README.md, "Adding a binding").

mod audio;
mod engine;
mod kag;
mod layer;
mod scene;
mod tjs;
mod window;
mod xp3;

/// Kirikiroid2 subsystem a function belongs to. Used to group the output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KirikiridModule {
    Engine,
    Tjs2,
    KagParser,
    XP3Archive,
    Visual,
    Audio,
}

impl KirikiridModule {
    pub fn title(self) -> &'static str {
        match self {
            Self::Engine => "Engine lifecycle (src/core/base/SysInitIntf, src/core/environ)",
            Self::Tjs2 => "TJS2 runtime (src/core/tjs2, src/core/base/ScriptMgnIntf)",
            Self::KagParser => "KAG3 parser (src/core/utils/KAGParser)",
            Self::XP3Archive => "Archives (src/core/base/XP3Archive, StorageIntf)",
            Self::Visual => "Layers / bitmaps / Window (src/core/visual)",
            Self::Audio => "Sound output (src/core/sound, environ/bevy/BevySound.cpp)",
        }
    }
}

/// One function parameter: C type and the type bindgen produces for it.
#[derive(Debug, Clone, Copy)]
pub struct Param {
    pub name: &'static str,
    pub c_ty: &'static str,
    pub rust_ty: &'static str,
}

/// One `extern "C"` function.
#[derive(Debug, Clone, Copy)]
pub struct FnDecl {
    pub module: KirikiridModule,
    pub name: &'static str,
    pub doc: &'static str,
    pub c_ret: &'static str,
    /// Rust return type as bindgen emits it; empty for `void`.
    pub rust_ret: &'static str,
    pub params: &'static [Param],
    /// The Kirikiroid2 entry points the shim forwards to (documentation).
    pub cpp_target: &'static str,
    /// C++ statements of the shim body. The generator wraps them in
    /// `try { ... } KRKR_SHIM_CATCH`; on an exception the function records
    /// the message for `krkr_last_error` and returns [`FnDecl::failure_value`].
    pub cpp_body: &'static str,
}

impl FnDecl {
    /// Value returned when the body throws: `KRKR_ERROR` for result codes,
    /// `NULL` for pointers, `0` otherwise, nothing for `void`.
    pub fn failure_value(&self) -> Option<&'static str> {
        match self.c_ret {
            "void" => None,
            "krkr_result_t" => Some("KRKR_ERROR"),
            r if r.ends_with('*') => Some("NULL"),
            _ => Some("0"),
        }
    }
}

pub(crate) const fn p(name: &'static str, c_ty: &'static str, rust_ty: &'static str) -> Param {
    Param {
        name,
        c_ty,
        rust_ty,
    }
}

pub(crate) const fn cstr(name: &'static str) -> Param {
    p(name, "const char*", "*const c_char")
}

pub(crate) const TJS: Param = p("tjs", "tTJSOpaque*", "*mut tTJSOpaque");
pub(crate) const PARSER: Param = p("parser", "KAGParserOpaque*", "*mut KAGParserOpaque");
pub(crate) const ARCHIVE: Param = p("archive", "XP3ArchiveOpaque*", "*mut XP3ArchiveOpaque");
pub(crate) const LAYER: Param = p("layer", "KrkrLayerOpaque*", "*mut KrkrLayerOpaque");

/// `(name, value, doc)` for `#define` result codes.
pub const CONSTANTS: &[(&str, i32, &str)] = &[
    ("KRKR_OK", 0, "Success."),
    ("KRKR_ERROR", -1, "Failure; krkr_last_error() describes it."),
    ("KRKR_END", 1, "The KAG scenario has no more tags."),
];

/// `(name, doc)` for opaque handle types.
pub const OPAQUE_TYPES: &[(&str, &str)] = &[
    ("tTJSOpaque", "Handle to the engine's TJS2 interpreter."),
    ("KAGParserOpaque", "Handle to a KAGParser object."),
    ("XP3ArchiveOpaque", "Handle to an opened XP3 (or TAR) archive."),
    ("KrkrLayerOpaque", "Handle to a Layer, Bitmap or BitmapLayerTreeOwner."),
];

/// A struct the ABI passes by pointer.
#[derive(Debug, Clone, Copy)]
pub struct StructDecl {
    pub name: &'static str,
    pub doc: &'static str,
    /// Fields as written in C, one line each.
    pub c_fields: &'static [&'static str],
    /// The same fields as bindgen spells them in Rust.
    pub rust_fields: &'static [&'static str],
}

pub const STRUCTS: &[StructDecl] = &[];

/// A function-pointer typedef, spelled for C and for Rust (as bindgen emits it).
#[derive(Debug, Clone, Copy)]
pub struct CallbackType {
    pub name: &'static str,
    pub doc: &'static str,
    pub c_params: &'static str,
    pub rust_params: &'static str,
}

pub const CALLBACK_TYPES: &[CallbackType] = &[
    CallbackType {
        name: "krkr_tag_callback_t",
        doc: "KAG tag callback. `attrs_json` is a JSON object whose values are all strings. \
              Text runs are tag \"ch\" with attribute \"text\".",
        c_params: "const char* tag_name, const char* attrs_json, void* userdata",
        rust_params: "tag_name: *const c_char, attrs_json: *const c_char, userdata: *mut c_void",
    },
    CallbackType {
        name: "krkr_log_callback_t",
        doc: "Engine log callback. `message` is UTF-8; `important` is non-zero for warnings and errors.",
        c_params: "const char* message, int important, void* userdata",
        rust_params: "message: *const c_char, important: c_int, userdata: *mut c_void",
    },

];

/// Every function, grouped by subsystem in output order.
pub const GROUPS: &[&[FnDecl]] = &[
    engine::FUNCTIONS,
    tjs::FUNCTIONS,
    kag::FUNCTIONS,
    xp3::FUNCTIONS,
    layer::FUNCTIONS,
    window::FUNCTIONS,
    scene::FUNCTIONS,
    audio::FUNCTIONS,
];

/// Iterate over every function in output order.
pub fn functions() -> impl Iterator<Item = &'static FnDecl> {
    GROUPS.iter().flat_map(|g| g.iter())
}
