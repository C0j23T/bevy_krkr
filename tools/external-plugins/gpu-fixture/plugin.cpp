#include "tp_stub.h"

namespace {
using Initialize = tjs_uint32 (TJS_INTF_METHOD *)(const tjs_char *, const char *);
using Apply = void (TJS_INTF_METHOD *)(const tjs_char *, iTJSDispatch2 *, iTJSDispatch2 *,
                                     const tTVPRect &, const tTVPRect &, const float *, tjs_uint);
Apply apply;
tjs_int references;

class PixelMethod final : public tTJSDispatch {
    tjs_error TJS_INTF_METHOD FuncCall(tjs_uint32, const tjs_char *member, tjs_uint32 *,
                                      tTJSVariant *, tjs_int count, tTJSVariant **params,
                                      iTJSDispatch2 *) override {
        if (member) return TJS_E_MEMBERNOTFOUND;
        if (count < 2) return TJS_E_BADPARAMCOUNT;
        const float values[] = {0, 128.0f / 255.0f};
        apply(TJS_W("GpuFixturePixel"), params[0]->AsObjectNoAddRef(),
              params[1]->AsObjectNoAddRef(), tTVPRect(1, 0, 3, 2), tTVPRect(0, 0, 2, 2),
              values, 2);
        return TJS_S_OK;
    }
};

const char wesl[] = R"(
    import package::krkr::operations::{p, source, coordinate, sample};
    public fn render(at: vec2<f32>, xy: vec2<i32>) -> vec4<f32> {
        let color = sample(source, coordinate(0u, at));
        return vec4(color.bg, p[2].y, color.a);
    }
)";
}

extern "C" __attribute__((visibility("default"))) tjs_error V2Link(iTVPFunctionExporter *exporter) {
    TVPInitImportStub(exporter);
    auto initialize = reinterpret_cast<Initialize>(TVPGetImportFuncPtr(
        "tjs_uint32 ::TVPInitializeGPU(const tjs_char *,const char *)"));
    apply = reinterpret_cast<Apply>(TVPGetImportFuncPtr(
        "void ::TVPApplyGPU(const tjs_char *,iTJSDispatch2 *,iTJSDispatch2 *,const tTVPRect &,const tTVPRect &,const float *,tjs_uint)"));
    auto opcode = initialize(TJS_W("GpuFixturePixel"), wesl);
    auto global = TVPGetScriptDispatch();
    auto callback = new PixelMethod();
    tTJSVariant function(callback, callback);
    callback->Release();
    global->PropSet(TJS_MEMBERENSURE, TJS_W("gpuFixtureApply"), nullptr, &function, global);
    tTJSVariant value(static_cast<tjs_int64>(opcode));
    global->PropSet(TJS_MEMBERENSURE, TJS_W("gpuFixtureOpcode"), nullptr, &value, global);
    global->Release();
    function.Clear();
    references = TVPPluginGlobalRefCount;
    return TJS_S_OK;
}

extern "C" __attribute__((visibility("default"))) tjs_error V2Unlink() {
    if (TVPPluginGlobalRefCount > references) return TJS_E_FAIL;
    auto global = TVPGetScriptDispatch();
    global->DeleteMember(0, TJS_W("gpuFixtureApply"), nullptr, global);
    global->DeleteMember(0, TJS_W("gpuFixtureOpcode"), nullptr, global);
    global->Release();
    TVPUninitImportStub();
    return TJS_S_OK;
}
