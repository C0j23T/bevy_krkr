#include "tp_stub.h"
bool onV2Link();
bool onV2Unlink();
extern "C" __attribute__((visibility("default"))) int32_t V2Link(iTVPFunctionExporter *exporter) {
    return TVPInitImportStub(exporter) && onV2Link() ? 0 : -1;
}
extern "C" __attribute__((visibility("default"))) int32_t V2Unlink() {
    if (!onV2Unlink()) return -1;
    TVPUninitImportStub();
    return 0;
}
