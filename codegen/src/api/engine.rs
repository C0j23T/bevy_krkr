//! Engine lifecycle: init / tick / shutdown, logging, errors, memory.

use super::KirikiridModule::Engine;
use super::{cstr, p, FnDecl};

pub const FUNCTIONS: &[FnDecl] = &[
    FnDecl {
        module: Engine,
        name: "krkr_engine_init",
        doc: "Start the engine with `data_path` (a data folder or a data.xp3 file) as the project. \
              Initialises storage, the TJS2 engine with all native classes, fonts, pixel routines \
              and the plugins compiled into the core (e.g. `System.addFont`). One instance at a \
              time: after krkr_engine_shutdown another instance may be started.",
        c_ret: "krkr_result_t",
        rust_ret: "krkr_result_t",
        params: &[cstr("data_path")],
        cpp_target: "TVPInitScriptEngine + TVPInitFontNames + TVPBevyLoadInternalPlugins + TVPSystemInit \
                     (as tTVPApplication::StartApplication)",
        cpp_body: r#"
    if (!data_path) throw std::invalid_argument("data_path is NULL");
    if (krkr_engine_state == KrkrEngineState::Running) throw std::logic_error("the engine is already running");
    if (krkr_engine_state == KrkrEngineState::Failed)
        throw std::logic_error("a previous krkr_engine_init failed; the engine cannot be initialised again in this process");
    std::error_code ec;
    std::filesystem::path path = std::filesystem::weakly_canonical(std::filesystem::absolute(data_path, ec), ec);
    if (ec || !std::filesystem::exists(path)) throw std::runtime_error(std::string("data path not found: ") + data_path);
    std::string native = path.string();
    if (std::filesystem::is_directory(path) && native.back() != '/') native += '/';
    // Failing half-way leaves the engine unusable; never retry.
    krkr_engine_state = KrkrEngineState::Failed;
    // The exit request of a previous session does not carry over into this one.
    TVPTerminated = false;
    TVPTerminateCode = 0;
    TVPNativeProjectDir = krkr_ttstr(native.c_str());
    TVPProjectDir = TVPNormalizeStorageName(TVPNativeProjectDir);
    TVPInitScriptEngine();
    TVPInitFontNames();
    // Attaches the compiled-in plugins (System.addFont, ...) to their TJS
    // classes; tTVPApplication::StartApplication does the same through
    // TVPLoadPluigins.
    TVPBevyLoadInternalPlugins();
    TVPSystemInit(); // appends the archive delimiter for data.xp3
    TVPSystemControl = new tTVPSystemControl();
    krkr_engine_state = KrkrEngineState::Running;
    return KRKR_OK;"#,
    },
    FnDecl {
        module: Engine,
        name: "krkr_engine_exec_startup",
        doc: "Run the project's start-up scripts the way Kirikiroid2 does when it \
              launches a game: `patch.tjs` next to the data file (if present), then \
              `startup.tjs`, and `AfterStartup.tjs` afterwards. Games ship `patch.tjs` \
              beside `data.xp3` to fix up or extend the engine's classes -- Fal \
              hollow ataraxia, for one, defines `GetFileSize` there -- so a script \
              that runs after start-up may rely on it. Failures of `patch.tjs` are \
              reported by the engine itself; an error from `startup.tjs` is returned.",
        c_ret: "krkr_result_t",
        rust_ret: "krkr_result_t",
        params: &[],
        cpp_target: "TVPExecuteStartupScript (base/ScriptMgnIntf.cpp)",
        cpp_body: r#"
    krkr_require_running();
    TVPExecuteStartupScript();
    return KRKR_OK;"#,
    },
    FnDecl {
        module: Engine,
        name: "krkr_engine_shutdown",
        doc: "Shut the engine down. Release every handle (TJS, archive, layer) first. \
              The process may then start another instance with krkr_engine_init.",
        c_ret: "void",
        rust_ret: "",
        params: &[],
        cpp_target: "TVPSystemUninit",
        cpp_body: r#"
    if (krkr_engine_state == KrkrEngineState::ShutDown) {
        // The engine released itself (`System.exit`, a script exception) and
        // ran its own shutdown, plugins and all; only what the host keeps
        // outside it is left to close.
        delete TVPSystemControl;
        TVPSystemControl = nullptr;
        TVPBevyResetWindows();
        TVPBevyResetScene();
        TVPBevyResetSound();
        krkr_clear_session_state();
        return;
    }
    if (krkr_engine_state != KrkrEngineState::Running) return;
    krkr_engine_state = KrkrEngineState::ShutDown;
    // Tell the game's windows they are closing while their scripts still run:
    // that is where a KAG game saves its system variables (volumes, message
    // speed), and the next session reads them back instead of finding void.
    TVPBevyCloseWindows();
    // Uninit detaches the plugins while their TJS classes still exist (see
    // TVPBeforeSystemUninit): the detachment also clears the per-class
    // registration state, which would otherwise make the next session fail
    // with "Already registerd class".
    TVPSystemUninit();
    delete TVPSystemControl;
    TVPSystemControl = nullptr;
    // The Window objects died with the TJS engine.
    TVPBevyResetWindows();
    TVPBevyResetScene();
    // Anything still queued for playback belongs to the session that just ended.
    TVPBevyResetSound();
    krkr_clear_session_state();"#,
    },
    FnDecl {
        module: Engine,
        name: "krkr_engine_tick",
        doc: "Deliver pending engine events (timers, async triggers, continuous handlers). Call once per frame.",
        c_ret: "krkr_result_t",
        rust_ret: "krkr_result_t",
        params: &[],
        cpp_target: "tTVPApplication::ProcessMessages + tTVPSystemControl::ApplicationIdle",
        cpp_body: r#"
    krkr_require_running();
    // Parts of the engine hand work to the application object instead of doing
    // it on their own thread: the TJS Timer thread posts its notifications
    // through `tTVPApplication::PostUserMessage`, and `ProcessMessages` is what
    // runs them. Without it a Timer never fires, which is also why KAG's
    // `[wait time=...]` never resumes.
    Application->ProcessMessages();
    TVPSystemControl->ApplicationIdle();
    // Windows are repainted here: the host owns the message loop, so nothing
    // else would produce a frame for them.
    TVPBevyTickWindows();
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
        name: "krkr_engine_is_running",
        doc: "Non-zero while an instance is up. It turns zero when the session shuts down, and \
              also when the engine released itself (`System.exit`, a script exception): from \
              then on the interpreter is gone, the host's handles to its objects are stale, and \
              all that is left to do is close the session.",
        c_ret: "int",
        rust_ret: "c_int",
        params: &[],
        cpp_target: "shim-side engine state",
        cpp_body: r#"
    return krkr_engine_alive() ? 1 : 0;"#,
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
