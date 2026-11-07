//! Visual output: pixels of a Layer, Bitmap or BitmapLayerTreeOwner (src/core/visual).
//!
//! The host reads a snapshot of the pixels (RGBA8, top-down, tightly packed)
//! and uploads it as a texture. `krkr_layer_get_revision` takes the snapshot
//! and hashes it, so the revision only changes when the pixels did.

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
    KrkrLayerView view;
    if (!krkr_layer_view(v.AsObjectNoAddRef(), view))
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
    if (layer->obj) layer->obj->Release();
    delete layer;"#,
    },
    FnDecl {
        module: Visual,
        name: "krkr_layer_get_revision",
        doc: "Take a pixel snapshot and return its hash. Changes only when the pixels or size changed.",
        c_ret: "uint64_t",
        rust_ret: "u64",
        params: &[LAYER],
        cpp_target: "XXH3 over the snapshot",
        cpp_body: r#"
    krkr_require(layer);
    krkr_layer_refresh(layer);
    return layer->revision;"#,
    },
    FnDecl {
        module: Visual,
        name: "krkr_layer_get_width",
        doc: "Width of the current snapshot (taken by krkr_layer_get_revision, or now if none).",
        c_ret: "unsigned int",
        rust_ret: "c_uint",
        params: &[LAYER],
        cpp_target: "snapshot",
        cpp_body: r#"
    krkr_require(layer);
    if (!layer->has_snapshot) krkr_layer_refresh(layer);
    return layer->width;"#,
    },
    FnDecl {
        module: Visual,
        name: "krkr_layer_get_height",
        doc: "Height of the current snapshot.",
        c_ret: "unsigned int",
        rust_ret: "c_uint",
        params: &[LAYER],
        cpp_target: "snapshot",
        cpp_body: r#"
    krkr_require(layer);
    if (!layer->has_snapshot) krkr_layer_refresh(layer);
    return layer->height;"#,
    },
    FnDecl {
        module: Visual,
        name: "krkr_layer_get_pixels",
        doc: "Snapshot pixels: RGBA8 (Kirikiroid2 renders in RGBA8888 byte order), top-down, \
              width*height*4 bytes, rows tightly packed. Alpha is 255 for a BitmapLayerTreeOwner \
              (opaque screen). Valid until the next call on this layer.",
        c_ret: "const uint8_t*",
        rust_ret: "*const u8",
        params: &[LAYER],
        cpp_target: "tTJSNI_Bitmap / tTJSNI_BaseLayer pixel buffer",
        cpp_body: r#"
    krkr_require(layer);
    if (!layer->has_snapshot) krkr_layer_refresh(layer);
    return layer->pixels.empty() ? nullptr : layer->pixels.data();"#,
    },
];
