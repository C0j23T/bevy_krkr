//! TJS2: the engine's interpreter (TVPGetScriptEngine), script and expression execution.

use super::KirikiridModule::Tjs2;
use super::{cstr, p, FnDecl, TJS};

pub const FUNCTIONS: &[FnDecl] = &[
    FnDecl {
        module: Tjs2,
        name: "krkr_tjs_create",
        doc: "Handle to the engine's TJS2 interpreter (all native classes registered). \
              Requires krkr_engine_init. Release with krkr_tjs_destroy before krkr_engine_shutdown.",
        c_ret: "tTJSOpaque*",
        rust_ret: "*mut tTJSOpaque",
        params: &[],
        cpp_target: "TVPGetScriptEngine + tTJS::AddRef",
        cpp_body: r#"
    krkr_require_running();
    tTJS *tjs = TVPGetScriptEngine();
    if (!tjs) throw std::runtime_error("the TJS2 engine is not initialised");
    tjs->AddRef();
    return reinterpret_cast<tTJSOpaque *>(tjs);"#,
    },
    FnDecl {
        module: Tjs2,
        name: "krkr_tjs_destroy",
        doc: "Release a handle from krkr_tjs_create.",
        c_ret: "void",
        rust_ret: "",
        params: &[TJS],
        cpp_target: "tTJS::Release",
        cpp_body: r#"
    if (tjs) reinterpret_cast<tTJS *>(tjs)->Release();"#,
    },
    FnDecl {
        module: Tjs2,
        name: "krkr_tjs_exec_script",
        doc: "Execute TJS2 source in the global context. `context_name` names it in error messages.",
        c_ret: "krkr_result_t",
        rust_ret: "krkr_result_t",
        params: &[TJS, cstr("script"), cstr("context_name")],
        cpp_target: "TVPExecuteScript",
        cpp_body: r#"
    krkr_require_running();
    krkr_require(tjs && script);
    TVPExecuteScript(krkr_ttstr(script), krkr_ttstr(context_name ? context_name : "bevy_krkr"), 0);
    return KRKR_OK;"#,
    },
    FnDecl {
        module: Tjs2,
        name: "krkr_tjs_eval",
        doc: "Evaluate an expression. If `out_result` is not NULL it receives the value converted \
              to a UTF-8 string; free it with krkr_free.",
        c_ret: "krkr_result_t",
        rust_ret: "krkr_result_t",
        params: &[TJS, cstr("expr"), p("out_result", "char**", "*mut *mut c_char")],
        cpp_target: "TVPExecuteExpression",
        cpp_body: r#"
    krkr_require_running();
    krkr_require(tjs && expr);
    if (out_result) *out_result = nullptr;
    tTJSVariant result;
    TVPExecuteExpression(krkr_ttstr(expr), &result);
    if (out_result) *out_result = krkr_strdup(krkr_variant_to_utf8(result));
    return KRKR_OK;"#,
    },
    FnDecl {
        module: Tjs2,
        name: "krkr_tjs_call",
        doc: "Call the function that `func_expr` evaluates to (e.g. \"obj.method\") with `nargs` \
              string arguments. The result is handled as in krkr_tjs_eval.",
        c_ret: "krkr_result_t",
        rust_ret: "krkr_result_t",
        params: &[
            TJS,
            cstr("func_expr"),
            p("args", "const char* const*", "*const *const c_char"),
            p("nargs", "size_t", "usize"),
            p("out_result", "char**", "*mut *mut c_char"),
        ],
        cpp_target: "TVPExecuteExpression + tTJSVariantClosure::FuncCall",
        cpp_body: r#"
    krkr_require_running();
    krkr_require(tjs && func_expr && (args || nargs == 0));
    if (out_result) *out_result = nullptr;
    tTJSVariant func;
    TVPExecuteExpression(krkr_ttstr(func_expr), &func);
    if (func.Type() != tvtObject) throw std::runtime_error(std::string(func_expr) + " is not a function");
    std::vector<tTJSVariant> values;
    values.reserve(nargs);
    for (size_t i = 0; i < nargs; ++i) values.emplace_back(krkr_ttstr(args[i] ? args[i] : ""));
    std::vector<tTJSVariant *> ptrs;
    for (auto &v : values) ptrs.push_back(&v);
    tTJSVariantClosure clo = func.AsObjectClosureNoAddRef();
    tTJSVariant result;
    tjs_error hr = clo.FuncCall(0, nullptr, nullptr, &result, (tjs_int)nargs, ptrs.data(), nullptr);
    if (TJS_FAILED(hr)) TJSThrowFrom_tjs_error(hr, krkr_ttstr(func_expr).c_str());
    if (out_result) *out_result = krkr_strdup(krkr_variant_to_utf8(result));
    return KRKR_OK;"#,
    },
];
