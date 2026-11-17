#include <tp_stub.h>

#undef V2Link
#undef V2Unlink

extern "C" HRESULT STDCALL PSDFileLink(iTVPFunctionExporter *exporter);
extern "C" HRESULT STDCALL PSDFileUnlink();

static unsigned links = 0;

extern "C" __attribute__((visibility("default"))) HRESULT STDCALL
V2Link(iTVPFunctionExporter *exporter) {
    if (links == 0) {
        const HRESULT result = PSDFileLink(exporter);
        if (TJS_FAILED(result)) return result;
    }
    ++links;
    return S_OK;
}

extern "C" __attribute__((visibility("default"))) HRESULT STDCALL V2Unlink() {
    if (links == 0) return E_FAIL;
    if (links == 1) {
        const HRESULT result = PSDFileUnlink();
        if (TJS_FAILED(result)) return result;
    }
    --links;
    return S_OK;
}
