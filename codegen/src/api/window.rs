//! Kirikiri Window input.

use super::KirikiridModule::Visual;
use super::{p, FnDecl};

pub const FUNCTIONS: &[FnDecl] = &[
    FnDecl {
        module: Visual,
        name: "krkr_window_mouse_move",
        doc:
            "Post a mouse-move event to the front-most visible Window. `x`/`y` are window pixels. \
              Nothing happens when no window is visible.",
        c_ret: "void",
        rust_ret: "",
        params: &[p("x", "int", "c_int"), p("y", "int", "c_int")],
        cpp_target: "tTVPOnMouseMoveInputEvent",
        cpp_body: r#"
    TVPBevyWindowMouseMove(x, y);"#,
    },
    FnDecl {
        module: Visual,
        name: "krkr_window_mouse_down",
        doc: "Post a mouse-button-down event. `button`: 0 left, 1 right, 2 middle.",
        c_ret: "void",
        rust_ret: "",
        params: &[
            p("x", "int", "c_int"),
            p("y", "int", "c_int"),
            p("button", "int", "c_int"),
        ],
        cpp_target: "tTVPOnMouseDownInputEvent",
        cpp_body: r#"
    TVPBevyWindowMouseDown(x, y, button);"#,
    },
    FnDecl {
        module: Visual,
        name: "krkr_window_mouse_up",
        doc: "Post a mouse-button-up event. `button`: 0 left, 1 right, 2 middle.",
        c_ret: "void",
        rust_ret: "",
        params: &[
            p("x", "int", "c_int"),
            p("y", "int", "c_int"),
            p("button", "int", "c_int"),
        ],
        cpp_target: "tTVPOnMouseUpInputEvent",
        cpp_body: r#"
    TVPBevyWindowMouseUp(x, y, button);"#,
    },
    FnDecl {
        module: Visual,
        name: "krkr_window_key_down",
        doc: "Post a key-down event. `vk` is a Win32 virtual-key code, as Kirikiri scripts use \
              (13 = Enter, 32 = Space, 27 = Escape); `shift` is the ss* bit set.",
        c_ret: "void",
        rust_ret: "",
        params: &[
            p("vk", "int", "c_int"),
            p("shift", "unsigned int", "c_uint"),
        ],
        cpp_target: "tTVPOnKeyDownInputEvent",
        cpp_body: r#"
    TVPBevyWindowKeyDown(vk, shift);"#,
    },
    FnDecl {
        module: Visual,
        name: "krkr_window_key_up",
        doc: "Post a key-up event; see krkr_window_key_down.",
        c_ret: "void",
        rust_ret: "",
        params: &[
            p("vk", "int", "c_int"),
            p("shift", "unsigned int", "c_uint"),
        ],
        cpp_target: "tTVPOnKeyUpInputEvent",
        cpp_body: r#"
    TVPBevyWindowKeyUp(vk, shift);"#,
    },
];
