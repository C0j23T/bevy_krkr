use super::KirikiridModule::Visual;
use super::{p, FnDecl, LAYER};

pub const FUNCTIONS: &[FnDecl] = &[
    FnDecl {
        module: Visual,
        name: "krkr_scene_build",
        doc: "Submit the visible layer tree to Bevy and return its GPU texture identity. Zero means no visible scene.",
        c_ret: "uint64_t",
        rust_ret: "u64",
        params: &[LAYER, p("width", "int*", "*mut c_int"), p("height", "int*", "*mut c_int")],
        cpp_target: "TVPBevyBuildScene",
        cpp_body: r#"
    krkr_require_running();
    return TVPBevyBuildScene(layer ? layer->obj : nullptr, width, height);"#,
    },
];
