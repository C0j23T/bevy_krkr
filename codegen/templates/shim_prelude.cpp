// ── shim support code (codegen/templates/shim_prelude.cpp) ──────────────────
// Shared helpers for the generated function bodies. Expanded verbatim into
// krkr_c_api.cpp by krkr_codegen, after the generated header include.

#include "tjsCommHead.h"
#include "tjsDictionary.h"
#include "tjsError.h"
#include "ScriptMgnIntf.h"
#include "SysInitIntf.h"
#include "SysInitImpl.h"
#include "StorageIntf.h"
#include "EventIntf.h"
#include "MsgIntf.h"
#include "BitmapIntf.h"
#include "LayerIntf.h"
#include "BitmapLayerTreeOwner.h"
#include "LayerManager.h"
#include "SystemControl.h"
#include "BevyPlatform.h"

#include <algorithm>
#include <cstdlib>
#include <cstring>
#include <filesystem>
#include <initializer_list>
#include <memory>
#include <stdexcept>
#include <string>
#include <vector>
#include <xxhash.h>

extern ttstr TVPNativeProjectDir;     // base/win32/SysInitImpl.cpp
extern void TVPInitFontNames();       // visual/FontImpl.cpp

// ─── errors ─────────────────────────────────────────────────────────────────

static thread_local std::string krkr_last_error_text;

static void krkr_set_error(std::string msg) { krkr_last_error_text = std::move(msg); }

// Every generated function body is a try block followed by
// KRKR_SHIM_CATCH(<failure value>).
#define KRKR_SHIM_CATCH(...)                                                                  \
    catch (const TVPBevyExitRequest &e) {                                                    \
        krkr_set_error("the engine requested exit (code " + std::to_string(e.code) + ")");  \
        return __VA_ARGS__;                                                                   \
    } catch (const eTJSScriptError &e) {                                                     \
        std::string trace = e.GetTrace().AsStdString();                                      \
        krkr_set_error(e.GetMessage().AsStdString() + (trace.empty() ? "" : " (at " + trace + ")")); \
        return __VA_ARGS__;                                                                   \
    } catch (const eTJS &e) {                                                                \
        krkr_set_error(e.GetMessage().AsStdString());                                        \
        return __VA_ARGS__;                                                                   \
    } catch (const std::exception &e) {                                                      \
        krkr_set_error(e.what());                                                            \
        return __VA_ARGS__;                                                                   \
    } catch (...) {                                                                          \
        krkr_set_error("unknown C++ exception");                                             \
        return __VA_ARGS__;                                                                   \
    }

static void krkr_require(bool ok) {
    if (!ok) throw std::invalid_argument("NULL argument");
}

// ─── engine state ───────────────────────────────────────────────────────────

// Running: a session is up. ShutDown: it was torn down and another one may
// start. Failed: initialisation threw half-way, which leaves globals in an
// unknown state; the engine is never retried in that process.
enum class KrkrEngineState { NotStarted, Running, ShutDown, Failed };
static KrkrEngineState krkr_engine_state = KrkrEngineState::NotStarted;

static void krkr_require_running() {
    if (krkr_engine_state != KrkrEngineState::Running) throw std::logic_error("the engine is not running");
}

// Drop everything the engine keeps outside the session's own objects. They all
// refer to the instance being shut down: pending events, continuous handlers
// and event hooks point into the TJS engine, cached archives and XP3 segments
// belong to its data path. Upstream clears them at process exit only.
static void krkr_clear_session_state() {
    TVPUninitEventState();      // EventIntf.h
    TVPClearStorageArchiveCache(); // StorageIntf.h
    TVPClearStorageCaches();
}

// ─── strings ────────────────────────────────────────────────────────────────

// Narrow strings are UTF-8 in Kirikiroid2 (TJS_mbstowcs uses utf8_mbtowc).
static ttstr krkr_ttstr(const char *utf8) { return ttstr(utf8 ? utf8 : ""); }

static std::string krkr_variant_to_utf8(const tTJSVariant &v) {
    if (v.Type() == tvtVoid) return std::string();
    return ttstr(v).AsStdString();
}

static char *krkr_strdup(const std::string &s) {
    char *p = (char *)std::malloc(s.size() + 1);
    if (!p) throw std::bad_alloc();
    std::memcpy(p, s.c_str(), s.size() + 1);
    return p;
}

static void krkr_json_escape(std::string &out, const std::string &s) {
    out += '"';
    for (unsigned char c : s) {
        switch (c) {
        case '"': out += "\\\""; break;
        case '\\': out += "\\\\"; break;
        case '\n': out += "\\n"; break;
        case '\r': out += "\\r"; break;
        case '\t': out += "\\t"; break;
        default:
            if (c < 0x20) {
                char buf[8];
                std::snprintf(buf, sizeof buf, "\\u%04x", c);
                out += buf;
            } else {
                out += (char)c;
            }
        }
    }
    out += '"';
}

// ─── logging ────────────────────────────────────────────────────────────────

static krkr_log_callback_t krkr_log_cb = nullptr;
static void *krkr_log_ud = nullptr;

static void krkr_log_adapter(const char *utf8, bool important, void *) {
    if (krkr_log_cb) krkr_log_cb(utf8, important ? 1 : 0, krkr_log_ud);
}

// ─── TJS helpers ────────────────────────────────────────────────────────────

// Call obj.method(args...) and return the result. Throws on failure.
static tTJSVariant krkr_call_method(iTJSDispatch2 *obj, const tjs_char *method,
                                    std::initializer_list<tTJSVariant *> args) {
    std::vector<tTJSVariant *> params(args);
    tTJSVariant result;
    tjs_error hr = obj->FuncCall(0, method, nullptr, &result, (tjs_int)params.size(), params.data(), obj);
    if (TJS_FAILED(hr)) TJSThrowFrom_tjs_error(hr, method);
    return result;
}

// Serialise a dictionary to a JSON object of strings, skipping `skip`.
struct KrkrJsonCollector : public tTJSDispatch {
    std::string json = "{";
    bool first = true;
    const tjs_char *skip = nullptr;

    tjs_error TJS_INTF_METHOD FuncCall(tjs_uint32, const tjs_char *, tjs_uint32 *, tTJSVariant *result,
                                       tjs_int numparams, tTJSVariant **param, iTJSDispatch2 *) override {
        // EnumMembers calls back with (name, flags, value).
        if (numparams < 3) return TJS_E_BADPARAMCOUNT;
        if (result) *result = (tjs_int)1; // continue enumeration
        if ((tjs_int)*param[1] & TJS_HIDDENMEMBER) return TJS_S_OK;
        ttstr key(*param[0]);
        if (skip && key == skip) return TJS_S_OK;
        if (!first) json += ',';
        first = false;
        krkr_json_escape(json, key.AsStdString());
        json += ':';
        krkr_json_escape(json, krkr_variant_to_utf8(*param[2]));
        return TJS_S_OK;
    }
};

static std::string krkr_dict_to_json(iTJSDispatch2 *dict, const tjs_char *skip) {
    KrkrJsonCollector collector;
    collector.skip = skip;
    tTJSVariantClosure clo(&collector, nullptr);
    dict->EnumMembers(TJS_IGNOREPROP, &clo, dict);
    collector.json += '}';
    return collector.json;
}

// ─── opaque handles ─────────────────────────────────────────────────────────

struct KAGParserOpaque {
    iTJSDispatch2 *obj = nullptr; // a KAGParser instance, owned
    krkr_tag_callback_t callback = nullptr;
    void *userdata = nullptr;
};

struct XP3ArchiveOpaque {
    tTVPArchive *arc = nullptr; // owned (Release)
};

struct KrkrLayerOpaque {
    iTJSDispatch2 *obj = nullptr; // Layer, Bitmap or BitmapLayerTreeOwner, owned
    std::vector<uint8_t> pixels;  // last snapshot, RGBA8 top-down packed
    unsigned int width = 0, height = 0;
    uint64_t revision = 0;
    bool has_snapshot = false;
};

// Where the pixels of a pixel-bearing TJS object live. Rows are read one by
// one with GetScanLine(y): software textures may store rows compressed or
// shared, so a single base pointer plus pitch is not enough.
struct KrkrLayerView {
    const tTVPNativeBaseBitmap *bmp = nullptr; // NULL: no pixels yet
    unsigned int width = 0, height = 0;        // visible size (<= bitmap size)
    bool opaque = false;                       // force alpha to 255 (composited screen)
};

static void krkr_bitmap_view(const tTVPNativeBaseBitmap *bmp, KrkrLayerView &view) {
    view.bmp = bmp;
    view.width = bmp ? bmp->GetWidth() : 0;
    view.height = bmp ? bmp->GetHeight() : 0;
}

// Recognise the object by its native instance. Returns false if it is not a
// Bitmap, a Layer or a BitmapLayerTreeOwner.
static bool krkr_layer_view(iTJSDispatch2 *obj, KrkrLayerView &view) {
    iTJSNativeInstance *ni = nullptr;
    if (TJS_SUCCEEDED(obj->NativeInstanceSupport(TJS_NIS_GETINSTANCE, tTJSNC_BitmapLayerTreeOwner::ClassID, &ni)) && ni) {
        // Kirikiroid2 composites the layer tree into the LayerManager's draw
        // buffer (tTVPLayerManager::DrawCompleted) and its window shows that
        // buffer (tTVPBasicDrawDevice::Show); the owner's own Bitmap is not
        // updated. Read the draw buffer, cropped to the primary layer size.
        auto *owner = static_cast<tTJSNI_BitmapLayerTreeOwner *>(ni);
        view.opaque = true;
        tTJSNI_BaseLayer *primary = owner->GetPrimaryLayer();
        tTVPLayerManager *manager = primary ? primary->GetManager() : nullptr;
        tjs_int w = 0, h = 0;
        if (!manager || !manager->GetPrimaryLayerSize(w, h)) return true; // no primary layer yet
        krkr_bitmap_view(manager->GetDrawBuffer(), view);
        if (view.bmp) {
            view.width = std::min<unsigned int>(view.width, (unsigned int)w);
            view.height = std::min<unsigned int>(view.height, (unsigned int)h);
        }
        return true;
    }
    ni = nullptr;
    if (TJS_SUCCEEDED(obj->NativeInstanceSupport(TJS_NIS_GETINSTANCE, tTJSNC_Bitmap::ClassID, &ni)) && ni) {
        krkr_bitmap_view(static_cast<tTJSNI_Bitmap *>(ni)->GetBitmap(), view);
        return true;
    }
    ni = nullptr;
    if (TJS_SUCCEEDED(obj->NativeInstanceSupport(TJS_NIS_GETINSTANCE, tTJSNC_Layer::ClassID, &ni)) && ni) {
        auto *lay = dynamic_cast<tTJSNI_BaseLayer *>(ni);
        if (!lay) return false;
        krkr_bitmap_view(lay->GetMainImage(), view);
        return true;
    }
    return false;
}

// Copy the object's pixels into the handle and recompute the revision.
static void krkr_layer_refresh(KrkrLayerOpaque *layer) {
    if (krkr_engine_state != KrkrEngineState::Running) return; // keep the last snapshot
    KrkrLayerView view;
    if (!krkr_layer_view(layer->obj, view) || !view.bmp || !view.bmp->Is32BPP()) view = KrkrLayerView();
    layer->width = view.width;
    layer->height = view.height;
    size_t row = (size_t)layer->width * 4;
    layer->pixels.resize(row * layer->height);
    for (unsigned int y = 0; y < layer->height; ++y) {
        uint8_t *dst = layer->pixels.data() + row * y;
        std::memcpy(dst, view.bmp->GetScanLine(y), row);
        if (view.opaque)
            for (size_t x = 3; x < row; x += 4) dst[x] = 0xff;
    }
    uint64_t seed = ((uint64_t)layer->width << 32) | layer->height;
    layer->revision = XXH3_64bits_withSeed(layer->pixels.data(), layer->pixels.size(), seed);
    layer->has_snapshot = true;
}
