//! Engine lifecycle: init / tick / shutdown, logging, errors, memory.

use super::KirikiridModule::Engine;
use super::{cstr, p, FnDecl};

pub const FUNCTIONS: &[FnDecl] = &[
    FnDecl {
        module: Engine,
        name: "krkr_engine_init",
        doc: "Start the engine with `data_path` (a data folder or a data.xp3 file) as the project. \
              Initialises storage, the TJS2 engine with all native classes, fonts and pixel \
              routines. Once per process: the engine cannot be restarted after krkr_engine_shutdown.",
        c_ret: "krkr_result_t",
        rust_ret: "krkr_result_t",
        params: &[cstr("data_path")],
        cpp_target: "TVPInitScriptEngine + TVPInitFontNames + TVPSystemInit (as tTVPApplication::StartApplication)",
        cpp_body: r#"
    if (!data_path) throw std::invalid_argument("data_path is NULL");
    if (krkr_engine_state == KrkrEngineState::Running) throw std::logic_error("the engine is already running");
    if (krkr_engine_state == KrkrEngineState::ShutDown)
        throw std::logic_error("the engine cannot be restarted in the same process");
    std::error_code ec;
    std::filesystem::path path = std::filesystem::weakly_canonical(std::filesystem::absolute(data_path, ec), ec);
    if (ec || !std::filesystem::exists(path)) throw std::runtime_error(std::string("data path not found: ") + data_path);
    std::string native = path.string();
    if (std::filesystem::is_directory(path) && native.back() != '/') native += '/';
    // Failing half-way leaves the engine unusable; never retry.
    krkr_engine_state = KrkrEngineState::ShutDown;
    TVPNativeProjectDir = krkr_ttstr(native.c_str());
    TVPProjectDir = TVPNormalizeStorageName(TVPNativeProjectDir);
    TVPInitScriptEngine();
    TVPInitFontNames();
    TVPSystemInit(); // appends the archive delimiter for data.xp3
    TVPSystemControl = new tTVPSystemControl();
    krkr_engine_state = KrkrEngineState::Running;
    return KRKR_OK;"#,
    },
    FnDecl {
        module: Engine,
        name: "krkr_engine_shutdown",
        doc: "Shut the engine down. Release every handle (TJS, parser, archive, layer) first.",
        c_ret: "void",
        rust_ret: "",
        params: &[],
        cpp_target: "TVPSystemUninit",
        cpp_body: r#"
    if (krkr_engine_state != KrkrEngineState::Running) return;
    krkr_engine_state = KrkrEngineState::ShutDown;
    TVPSystemUninit();
    delete TVPSystemControl;
    TVPSystemControl = nullptr;"#,
    },
    FnDecl {
        module: Engine,
        name: "krkr_engine_tick",
        doc: "Deliver pending engine events (timers, async triggers, continuous handlers). Call once per frame.",
        c_ret: "krkr_result_t",
        rust_ret: "krkr_result_t",
        params: &[],
        cpp_target: "tTVPSystemControl::ApplicationIdle",
        cpp_body: r#"
    krkr_require_running();
    TVPSystemControl->ApplicationIdle();
    return KRKR_OK;"#,
    },
    FnDecl {
        module: Engine,
        name: "krkr_engine_exit_requested",
        doc: "Non-zero once script code has asked the engine to terminate (System.terminate / exit).",
        c_ret: "int",
        rust_ret: "c_int",
        params: &[],
        cpp_target: "TVPTerminated",
        cpp_body: r#"
    return TVPTerminated ? 1 : 0;"#,
    },
    FnDecl {
        module: Engine,
        name: "krkr_engine_set_log_callback",
        doc: "Receive every engine log line. NULL restores logging to stderr. May be called before krkr_engine_init.",
        c_ret: "void",
        rust_ret: "",
        params: &[
            p("callback", "krkr_log_callback_t", "krkr_log_callback_t"),
            p("userdata", "void*", "*mut c_void"),
        ],
        cpp_target: "TVPBevySetLogSink (environ/bevy/BevyPlatform.h)",
        cpp_body: r#"
    krkr_log_cb = callback;
    krkr_log_ud = userdata;
    TVPBevySetLogSink(callback ? &krkr_log_adapter : nullptr, nullptr);"#,
    },
    FnDecl {
        module: Engine,
        name: "krkr_engine_set_screen_size",
        doc: "Screen size reported to scripts (System.screenWidth/Height, desktop size). Default 1280x720.",
        c_ret: "void",
        rust_ret: "",
        params: &[p("width", "int", "c_int"), p("height", "int", "c_int")],
        cpp_target: "TVPBevySetScreenSize (environ/bevy/BevyPlatform.h)",
        cpp_body: r#"
    TVPBevySetScreenSize(width, height);"#,
    },
    FnDecl {
        module: Engine,
        name: "krkr_last_error",
        doc: "Message of the last failed call on this thread (UTF-8), or NULL. \
              Only meaningful right after a call reported failure; valid until the next call.",
        c_ret: "const char*",
        rust_ret: "*const c_char",
        params: &[],
        cpp_target: "shim-side thread-local",
        cpp_body: r#"
    return krkr_last_error_text.empty() ? nullptr : krkr_last_error_text.c_str();"#,
    },
    FnDecl {
        module: Engine,
        name: "krkr_free",
        doc: "Free a buffer or string returned by this API (krkr_tjs_eval, krkr_tjs_call, krkr_xp3_read_file).",
        c_ret: "void",
        rust_ret: "",
        params: &[p("ptr", "void*", "*mut c_void")],
        cpp_target: "free",
        cpp_body: r#"
    std::free(ptr);"#,
    },
];
