#include <windows.h>
#include <gdiplus.h>
#include <gdiplusflat.h>
#include <stdio.h>

struct playback {
    GpMetafile *metafile;
    int records, paths, pens, brushes, draws, errors;
};

static BOOL CALLBACK visit(EmfPlusRecordType type, UINT flags, UINT size,
                           const BYTE *data, void *context) {
    struct playback *playback = context;
    ++playback->records;
    if (type == EmfPlusRecordTypeObject) {
        switch ((flags >> 8) & 127) {
        case 1: ++playback->brushes; break;
        case 2: ++playback->pens; break;
        case 3: ++playback->paths; break;
        }
    }
    if (type == EmfPlusRecordTypeDrawPath || type == EmfPlusRecordTypeFillPath)
        ++playback->draws;
    Status status = GdipPlayMetafileRecord(playback->metafile, type, flags, size, data);
    if (status != Ok) {
        fprintf(stderr, "record %x returned %d\n", type, status);
        ++playback->errors;
    }
    return TRUE;
}

#define CHECK(expr) do { \
    Status status = (expr); \
    if (status != Ok) { fprintf(stderr, "%s returned %d\n", #expr, status); return 1; } \
} while (0)

static int render(const char *path, struct playback *playback, ARGB *pixels) {
    WCHAR filename[MAX_PATH];
    if (!MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, path, -1, filename, MAX_PATH))
        return 1;
    CHECK(GdipCreateMetafileFromFile(filename, &playback->metafile));
    GpBitmap *bitmap;
    GpGraphics *graphics;
    CHECK(GdipCreateBitmapFromScan0(64, 48, 0, PixelFormat32bppARGB, NULL, &bitmap));
    CHECK(GdipGetImageGraphicsContext((GpImage *)bitmap, &graphics));
    GpRectF dest = {0, 0, 64, 48};
    CHECK(GdipEnumerateMetafileDestRect(graphics, playback->metafile, &dest, visit, playback, NULL));
    for (int y = 0; y < 48; ++y)
        for (int x = 0; x < 64; ++x)
            CHECK(GdipBitmapGetPixel(bitmap, x, y, &pixels[y * 64 + x]));
    GdipDeleteGraphics(graphics);
    GdipDisposeImage((GpImage *)bitmap);
    GdipDisposeImage((GpImage *)playback->metafile);
    return playback->errors != 0;
}

int main(int argc, char **argv) {
    if (argc != 3)
        return 1;
    struct GdiplusStartupInput input = {1, NULL, FALSE, FALSE};
    ULONG_PTR token;
    CHECK(GdiplusStartup(&token, &input, NULL));
    struct playback original = {0}, exported = {0};
    ARGB reference[64 * 48], actual[64 * 48];
    if (render(argv[1], &original, reference) || render(argv[2], &exported, actual))
        return 1;
    GdiplusShutdown(token);

    int different = 0, covered = 0;
    for (int i = 0; i < 64 * 48; ++i) {
        different += reference[i] != actual[i];
        covered += reference[i] != reference[0];
    }
    printf("exported records %d paths %d pens %d brushes %d draws %d, differing pixels %d/%d, reference non-background pixels %d\n",
           exported.records, exported.paths, exported.pens, exported.brushes, exported.draws,
           different, 64 * 48, covered);
    printf("sample pixels %08x/%08x (background), %08x/%08x (20,30)\n",
           reference[0], actual[0], reference[30 * 64 + 20], actual[30 * 64 + 20]);
    return covered == 0 || different != 0 ||
           exported.paths != 2 || exported.pens != 1 ||
           exported.brushes != 1 || exported.draws != 2;
}
