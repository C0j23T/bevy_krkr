//! The Kirikiri `Window` class: the surface a game's own UI is built on.
//!
//! KAG's `KAGWindow` (`system/..`/`mainwindow.tjs`) creates a `Window` and puts
//! the message window, backgrounds and everything else in its layer tree. The
//! port renders it off-screen
//! (`Kirikiroid2/src/core/environ/bevy/BevyWindowLayer.cpp`) and shows the
//! front-most visible one, the same way it shows the script-created screen.
//!
//! Pixmaps and input both go through here: a snapshot of the composited window,
//! and the engine's own input events for the window's TJS handlers.

use super::KirikiridModule::Visual;
use super::{p, FnDecl};

pub const FUNCTIONS: &[FnDecl] = &[
    FnDecl {
        module: Visual,
        name: "krkr_window_snapshot",
        doc: "Pixels of the front-most visible Window, or NULL when there is none (or it has \
              not drawn yet). `width*height*4` bytes of RGBA8, top-down, tightly packed; valid \
              until the next call into the engine. The out-parameters are written only when a \
              pointer is returned.",
        c_ret: "const uint8_t*",
        rust_ret: "*const u8",
        params: &[
            p("out_width", "unsigned int*", "*mut c_uint"),
            p("out_height", "unsigned int*", "*mut c_uint"),
            p("out_revision", "uint64_t*", "*mut u64"),
        ],
        cpp_target: "TVPBevyGetWindowPixels (environ/bevy/BevyWindowLayer.cpp)",
        cpp_body: r#"
    TVPBevyWindowPixels view;
    if (!TVPBevyGetWindowPixels(view)) return nullptr;
    if (out_width) *out_width = view.width;
    if (out_height) *out_height = view.height;
    if (out_revision) *out_revision = view.revision;
    return view.pixels;"#,
    },
    FnDecl {
        module: Visual,
        name: "krkr_window_mouse_move",
        doc: "Post a mouse-move event to the front-most visible Window. `x`/`y` are window pixels. \
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
