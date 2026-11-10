//! Build manifest of the Kirikiroid2 engine core for the bevy port.
//!
//! Which upstream sources `krkr_sys/build.rs` compiles, with which include
//! directories, defines and system libraries. The source globs mirror
//! `src/core/Android.mk`, minus the subsystems bevy_krkr does not take from
//! the engine (cocos2d-x front end and UI, movie/ffmpeg, SDL mixer, 7z/zip,
//! formats whose libraries are not vendored). What replaces them lives in
//! `src/core/environ/bevy/` on the `bevy-port` branch of Kirikiroid2.

use std::io;
use std::path::{Path, PathBuf};

/// Exists only on the `bevy-port` branch; used to detect a usable checkout.
pub const PORT_MARKER: &str = "src/core/environ/bevy/BevyPlatform.h";

/// Root of the engine core inside a Kirikiroid2 checkout.
pub const CORE_DIR: &str = "src/core";

/// `(directory under src/core, extension, excluded file stems)`.
const SOURCE_GLOBS: &[(&str, &str, &[&str])] = &[
    ("tjs2", "cpp", &[]),
    ("base", "cpp", &["7zArchive", "ZIPArchive"]),
    ("base/win32", "cpp", &["FuncStubs", "SusieArchive", "FileSelector"]),
    ("environ", "cpp", &["MainFormUnit", "DumpSend", "XP3ArchiveRepack"]),
    ("environ/linux", "cpp", &[]),
    ("environ/ConfigManager", "cpp", &[]),
    ("environ/bevy", "cpp", &[]),
    ("extension", "cpp", &[]),
    ("msg", "cpp", &[]),
    ("sound", "cpp", &["xmmlib", "WaveFormatConverter_SSE", "FFWaveDecoder", "PhaseVocoderDSP"]),
    ("sound/win32", "cpp", &["WaveMixer"]),
    ("utils", "c", &[]),
    ("utils", "cpp", &[]),
    ("utils/encoding", "c", &[]),
    ("utils/win32", "cpp", &[]),
    ("visual", "cpp", &["Resampler", "LoadBPG", "LoadJXR", "LoadPVRv3", "LoadWEBP"]),
    ("visual/win32", "cpp", &["GDIFontRasterizer", "NativeFreeTypeFace", "TVPSysFont", "VSyncTimingThread", "TVPScreen"]),
];

/// Single files outside the globs (same as Android.mk).
const EXTRA_SOURCES: &[&str] = &[
    "visual/gl/blend_function.cpp",
    "environ/win32/SystemControl.cpp",
    "msg/win32/MsgImpl.cpp",
    "msg/win32/OptionsDesc.cpp",
    "../plugins/ncbind/ncbind.cpp",
    "../plugins/InternalPlugins.cpp",
];

#[derive(Debug, Clone, Copy)]
pub struct Plugin {
    pub source: &'static str,
    pub module: &'static str,
    pub internal: bool,
    pub feature: Option<&'static str>,
}

pub const PLUGINS: &[Plugin] = &[
    Plugin { source: "src/plugins/layerExPerspective.cpp", module: "perspective.dll", internal: false, feature: None },
    Plugin { source: "src/core/environ/bevy/plugins/LayerExRaster.cpp", module: "layerExRaster.dll", internal: false, feature: None },
    Plugin { source: "src/core/environ/bevy/plugins/LayerExSave.cpp", module: "layerExSave.dll", internal: false, feature: None },
    Plugin { source: "src/core/environ/bevy/plugins/LayerExMovie.cpp", module: "layerExMovie.dll", internal: false, feature: None },
    Plugin { source: "src/core/environ/bevy/plugins/LayerExImage.cpp", module: "layerExImage.dll", internal: false, feature: None },
    Plugin { source: "src/plugins/addFont.cpp", module: "addFont.dll", internal: false, feature: None },
    Plugin { source: "src/plugins/csvParser.cpp", module: "csvParser.dll", internal: false, feature: None },
    Plugin { source: "src/plugins/dirlist.cpp", module: "dirlist.dll", internal: false, feature: None },
    Plugin { source: "src/plugins/fftgraph.cpp", module: "fftgraph.dll", internal: false, feature: None },
    Plugin { source: "src/plugins/getabout.cpp", module: "getabout.dll", internal: false, feature: None },
    Plugin { source: "src/plugins/getSample.cpp", module: "getSample.dll", internal: false, feature: None },
    Plugin { source: "src/plugins/saveStruct.cpp", module: "saveStruct.dll", internal: false, feature: None },
    Plugin { source: "src/plugins/varfile.cpp", module: "varfile.dll", internal: false, feature: None },
    Plugin { source: "src/plugins/win32dialog.cpp", module: "win32dialog.dll", internal: false, feature: None },
    Plugin { source: "src/plugins/wutcwf.cpp", module: "wutcwf.dll", internal: false, feature: None },
    Plugin {
        source: "src/core/environ/bevy/plugins/ScriptsExt.cpp",
        module: "yz_scripts_ext.dll",
        internal: true,
        feature: None,
    },
    Plugin {
        source: "src/plugins/xp3filter.cpp",
        module: "xp3filter.dll",
        internal: true,
        feature: Some("plugin-xp3filter"),
    },
];

/// Whether an optional part of the build (`Some(feature)`) is in it, given the
/// features `krkr_sys` was built with.
pub fn enabled(feature: Option<&str>, feature_on: impl Fn(&str) -> bool) -> bool {
    feature.is_none_or(feature_on)
}


/// Include directories under src/core, in search order. `environ/bevy/compat`
/// comes first: it replaces headers of cocos2d-x and of files missing from the
/// open-source release.
pub const INCLUDE_DIRS: &[&str] = &[
    "environ/bevy/compat",
    ".",
    "tjs2",
    "base",
    "base/win32",
    "utils",
    "utils/win32",
    "msg",
    "msg/win32",
    "environ",
    "environ/win32",
    "environ/android",
    "environ/bevy",
    "visual",
    "visual/win32",
    "sound",
    "sound/win32",
    "extension",
    "../plugins",
];

/// Preprocessor defines (the first three as in Android.mk).
pub const DEFINES: &[&str] = &[
    "TJS_TEXT_OUT_CRLF",
    "__STDC_CONSTANT_MACROS",
    "USE_UNICODE_FSTRING",
    "KRKR_BEVY_PORT",
];

/// Defines a feature switches on: `(define, feature)`.
pub const FEATURE_DEFINES: &[(&str, &str)] = &[("KRKR_BEVY_OPUS", "opus")];

pub const FLAGS: &[&str] = &[
    // TJS2 represents empty strings as NULL tTJSVariantString pointers and
    // calls member functions on them, which test `if(!this)` (e.g.
    // tjsVariantString.cpp GetLength). GCC/Clang delete such checks when
    // optimising; this flag keeps them.
    "-fno-delete-null-pointer-checks",
    // The pixel routines (visual/tvpgl.cpp, LayerBitmapIntf.cpp) access byte
    // buffers through tjs_uint32 pointers.
    "-fno-strict-aliasing",
];

/// System libraries, as pkg-config package names, and the feature that needs
/// them (`None`: always). Every enabled one is linked.
pub const PKG_CONFIG: &[(&str, Option<&str>)] = &[
    ("freetype2", None),
    ("oniguruma", None),
    ("vorbisfile", None),
    ("opusfile", Some("opus")),
];

/// Sources of the engine core, split by language.
#[derive(Debug, Default)]
pub struct Sources {
    pub cpp: Vec<PathBuf>,
    pub c: Vec<PathBuf>,
}

/// List the sources of the engine core under `kirikiroid2_src`, sorted.
pub fn collect_sources(kirikiroid2_src: &Path) -> io::Result<Sources> {
    let core = kirikiroid2_src.join(CORE_DIR);
    let mut out = Sources::default();
    for (dir, ext, excluded) in SOURCE_GLOBS {
        let mut files = Vec::new();
        for entry in std::fs::read_dir(core.join(dir))? {
            let path = entry?.path();
            let matches_ext = path.extension().and_then(|e| e.to_str()) == Some(ext);
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
            if path.is_file() && matches_ext && !excluded.contains(&stem) {
                files.push(path);
            }
        }
        files.sort();
        if *ext == "c" { out.c.extend(files) } else { out.cpp.extend(files) }
    }
    for file in EXTRA_SOURCES {
        out.cpp.push(core.join(file));
    }
    Ok(out)
}

/// Absolute include directories for a checkout.
pub fn include_dirs(kirikiroid2_src: &Path) -> Vec<PathBuf> {
    let core = kirikiroid2_src.join(CORE_DIR);
    INCLUDE_DIRS.iter().map(|d| core.join(d)).collect()
}
