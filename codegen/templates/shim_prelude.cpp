// ── shim support code (codegen/templates/shim_prelude.cpp) ──────────────────
// Shared helpers for the generated function bodies. Expanded verbatim into
// krkr_c_api.cpp by krkr_codegen, after the generated header include.

#include "tjsCommHead.h"
#include "tjsDictionary.h"
#include "tjsError.h"
#include "Application.h"
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
#include <csetjmp>
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

/// Whether handles the host holds still point at live objects. They are TJS
/// objects, so once the engine has released itself (`System.exit`, a script
/// exception) only the handle structs are left.
static bool krkr_engine_alive() { return krkr_engine_state == KrkrEngineState::Running; }

// Drop what the engine keeps outside the session's own objects. Cached
// archives and XP3 segments belong to the session's data path; upstream clears
// them at process exit only. Queued events are not here: they reference TJS
// objects, so the engine drops them while those still exist (see
// TVPBeforeSystemUninit).
static void krkr_clear_session_state() {
    TVPClearStorageArchiveCache(); // StorageIntf.h
    TVPClearStorageCaches();
}

// ─── leaving the engine ─────────────────────────────────────────────────────

// `TVPExitApplication` is upstream's `exit()`: it never returns. A host cannot
// do that, but it cannot unwind the stack from there either. `System.exit`
// shuts the engine down first (`TVPTerminateSync` calls `TVPSystemUninit`) and
// only then exits, so every engine frame between it and the host is left over
// freed globals (`TJS` among them); unwinding through them runs cleanup code
// that dereferences them. The generated entry points therefore install a jump
// target and `TVPExitApplication` leaves through it, abandoning the call.
struct KrkrExitJump {
    std::jmp_buf Target;
    KrkrExitJump *Previous;
};

static thread_local KrkrExitJump *krkr_exit_jump = nullptr;

namespace {
/// Restores the enclosing jump target when a generated function returns.
class KrkrExitJumpRestore {
    KrkrExitJump *Previous;

public:
    explicit KrkrExitJumpRestore(KrkrExitJump *previous) : Previous(previous) {}
    ~KrkrExitJumpRestore() { krkr_exit_jump = Previous; }
    KrkrExitJumpRestore(const KrkrExitJumpRestore &) = delete;
    KrkrExitJumpRestore &operator=(const KrkrExitJumpRestore &) = delete;
};
} // namespace

/// Target of `TVPExitApplication` (environ/bevy/BevyPlatform.cpp). Returns
/// false when no entry point is waiting for it, in which case the caller has to
/// unwind the stack itself; otherwise it does not return.
bool TVPBevyLeaveThroughExitBoundary() {
    KrkrExitJump *jump = krkr_exit_jump;
    if (!jump) return false;
    // The engine has already released itself: from here on there is nothing to
    // call, only the session left to close.
    krkr_engine_state = KrkrEngineState::ShutDown;
    std::longjmp(jump->Target, 1);
}

// First statement of every generated body (see generate_cpp_shim).
#define KRKR_SHIM_ENTER(...)                                                                  \
    KrkrExitJump krkr_exit_jump_here;                                                         \
    krkr_exit_jump_here.Previous = krkr_exit_jump;                                            \
    krkr_exit_jump = &krkr_exit_jump_here;                                                    \
    KrkrExitJumpRestore krkr_exit_jump_restore(krkr_exit_jump_here.Previous);                  \
    if (setjmp(krkr_exit_jump_here.Target) != 0) {                                            \
        krkr_set_error("the engine requested exit (code " + std::to_string(TVPTerminateCode) + ")"); \
        return __VA_ARGS__;                                                                   \
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
    iTJSDispatch2 *obj = nullptr; // a Layer, Bitmap or BitmapLayerTreeOwner, owned
};

// Whether `obj` is one of the TJS objects that own pixels: a Bitmap, a Layer,
// or a BitmapLayerTreeOwner -- the layer tree `krkr_scene_build` walks.
static bool krkr_is_layer_object(iTJSDispatch2 *obj) {
    iTJSNativeInstance *ni = nullptr;
    if (TJS_SUCCEEDED(obj->NativeInstanceSupport(TJS_NIS_GETINSTANCE, tTJSNC_BitmapLayerTreeOwner::ClassID, &ni)) && ni)
        return true;
    ni = nullptr;
    if (TJS_SUCCEEDED(obj->NativeInstanceSupport(TJS_NIS_GETINSTANCE, tTJSNC_Bitmap::ClassID, &ni)) && ni)
        return true;
    ni = nullptr;
    if (TJS_SUCCEEDED(obj->NativeInstanceSupport(TJS_NIS_GETINSTANCE, tTJSNC_Layer::ClassID, &ni)) && ni)
        return true;
    return false;
}
