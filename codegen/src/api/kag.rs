//! KAG3: a KAGParser object (utils/KAGParser.cpp) driven tag by tag.

use super::KirikiridModule::KagParser;
use super::{cstr, p, FnDecl, PARSER, TJS};

pub const FUNCTIONS: &[FnDecl] = &[
    FnDecl {
        module: KagParser,
        name: "krkr_kag_create",
        doc: "Create a KAGParser object in the engine bound to `tjs`.",
        c_ret: "KAGParserOpaque*",
        rust_ret: "*mut KAGParserOpaque",
        params: &[TJS],
        cpp_target: "new KAGParser() (TVPCreateNativeClass_KAGParser)",
        cpp_body: r#"
    krkr_require_running();
    krkr_require(tjs);
    tTJSVariant obj;
    TVPExecuteExpression(ttstr(TJS_W("new KAGParser()")), &obj);
    if (obj.Type() != tvtObject) throw std::runtime_error("KAGParser is not available");
    KAGParserOpaque *parser = new KAGParserOpaque();
    parser->obj = obj.AsObject(); // AddRef'd
    return parser;"#,
    },
    FnDecl {
        module: KagParser,
        name: "krkr_kag_destroy",
        doc: "Release a parser from krkr_kag_create.",
        c_ret: "void",
        rust_ret: "",
        params: &[PARSER],
        cpp_target: "iTJSDispatch2::Release",
        cpp_body: r#"
    if (!parser) return;
    if (parser->obj && krkr_engine_alive()) parser->obj->Release();
    delete parser;"#,
    },
    FnDecl {
        module: KagParser,
        name: "krkr_kag_load_script",
        doc: "Load a scenario (e.g. \"first.ks\") from the project storage and start at its top.",
        c_ret: "krkr_result_t",
        rust_ret: "krkr_result_t",
        params: &[PARSER, cstr("storage")],
        cpp_target: "KAGParser.loadScenario",
        cpp_body: r#"
    krkr_require_running();
    krkr_require(parser && storage);
    tTJSVariant arg(krkr_ttstr(storage));
    krkr_call_method(parser->obj, TJS_W("loadScenario"), {&arg});
    return KRKR_OK;"#,
    },
    FnDecl {
        module: KagParser,
        name: "krkr_kag_jump",
        doc: "Load `storage` (NULL = keep the current scenario), then go to `label` \
              (NULL = the top). At least one must be given.",
        c_ret: "krkr_result_t",
        rust_ret: "krkr_result_t",
        params: &[PARSER, cstr("storage"), cstr("label")],
        cpp_target: "KAGParser.loadScenario + KAGParser.goToLabel",
        cpp_body: r#"
    krkr_require_running();
    krkr_require(parser && (storage || label));
    if (storage) {
        tTJSVariant arg(krkr_ttstr(storage));
        krkr_call_method(parser->obj, TJS_W("loadScenario"), {&arg});
    }
    if (label) {
        tTJSVariant arg(krkr_ttstr(label));
        krkr_call_method(parser->obj, TJS_W("goToLabel"), {&arg});
    }
    return KRKR_OK;"#,
    },
    FnDecl {
        module: KagParser,
        name: "krkr_kag_process_next",
        doc: "Parse the next tag and report it through the tag callback. \
              Returns KRKR_OK, KRKR_END (no more tags) or KRKR_ERROR.",
        c_ret: "krkr_result_t",
        rust_ret: "krkr_result_t",
        params: &[PARSER],
        cpp_target: "KAGParser.getNextTag (dictionary serialised to JSON)",
        cpp_body: r#"
    krkr_require_running();
    krkr_require(parser);
    tTJSVariant tag = krkr_call_method(parser->obj, TJS_W("getNextTag"), {});
    if (tag.Type() != tvtObject || !tag.AsObjectNoAddRef()) return KRKR_END;
    iTJSDispatch2 *dict = tag.AsObjectNoAddRef();
    tTJSVariant name;
    dict->PropGet(0, TJS_W("tagname"), nullptr, &name, dict);
    std::string json = krkr_dict_to_json(dict, TJS_W("tagname"));
    if (parser->callback)
        parser->callback(krkr_variant_to_utf8(name).c_str(), json.c_str(), parser->userdata);
    return KRKR_OK;"#,
    },
    FnDecl {
        module: KagParser,
        name: "krkr_kag_set_tag_callback",
        doc: "Install the tag callback used by krkr_kag_process_next.",
        c_ret: "void",
        rust_ret: "",
        params: &[
            PARSER,
            p("callback", "krkr_tag_callback_t", "krkr_tag_callback_t"),
            p("userdata", "void*", "*mut c_void"),
        ],
        cpp_target: "shim-side storage",
        cpp_body: r#"
    if (!parser) return;
    parser->callback = callback;
    parser->userdata = userdata;"#,
    },
];
