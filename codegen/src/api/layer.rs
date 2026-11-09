//! Handles to the objects that own pixels (src/core/visual).
//!
//! `krkr_layer_find` evaluates a TJS expression and returns a handle to the
//! Layer, Bitmap or BitmapLayerTreeOwner it yields. The host needs one for the
//! screen `host.tjs` builds; `krkr_scene_build` walks it.

use super::KirikiridModule::Visual;
use super::{cstr, FnDecl, LAYER};

pub const FUNCTIONS: &[FnDecl] = &[
    FnDecl {
        module: Visual,
        name: "krkr_layer_find",
        doc: "Evaluate `expr` (e.g. \"krkrBevy.screen\") and return a handle to the Layer, Bitmap \
              or BitmapLayerTreeOwner it yields; NULL if it is not one. Release with krkr_layer_release.",
        c_ret: "KrkrLayerOpaque*",
        rust_ret: "*mut KrkrLayerOpaque",
        params: &[cstr("expr")],
        cpp_target: "TVPExecuteExpression + NativeInstanceSupport(ClassID)",
        cpp_body: r#"
    krkr_require_running();
    krkr_require(expr);
    tTJSVariant v;
    TVPExecuteExpression(krkr_ttstr(expr), &v);
    if (v.Type() != tvtObject || !v.AsObjectNoAddRef()) throw std::runtime_error(std::string(expr) + " is not an object");
    if (!krkr_is_layer_object(v.AsObjectNoAddRef()))
        throw std::runtime_error(std::string(expr) + " is not a Layer, Bitmap or BitmapLayerTreeOwner");
    KrkrLayerOpaque *layer = new KrkrLayerOpaque();
    layer->obj = v.AsObject(); // AddRef'd
    return layer;"#,
    },
    FnDecl {
        module: Visual,
        name: "krkr_layer_release",
        doc: "Release a handle from krkr_layer_find.",
        c_ret: "void",
        rust_ret: "",
        params: &[LAYER],
        cpp_target: "iTJSDispatch2::Release",
        cpp_body: r#"
    if (!layer) return;
    if (layer->obj && krkr_engine_alive()) layer->obj->Release();
    delete layer;"#,
    },
];
