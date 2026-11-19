#include <windows.h>
#include <gdiplus.h>
#include <gdiplusflat.h>
#include <stdio.h>

int main(int argc, char **argv) {
    if (argc != 2) return 1;
    struct GdiplusStartupInput input = {1, NULL, FALSE, FALSE};
    ULONG_PTR token;
    GpBitmap *bitmap;
    GpGraphics *graphics;
    if (GdiplusStartup(&token, &input, NULL) ||
        GdipCreateBitmapFromScan0(8, 8, 0, PixelFormat32bppARGB, NULL, &bitmap) ||
        GdipGetImageGraphicsContext((GpImage *)bitmap, &graphics)) return 1;
    GdipSetPageUnit(graphics, UnitPixel);
    FILE *out = fopen(argv[1], "wb");
    if (!out) return 1;
    for (int style = 0; style < 53; ++style) {
        GpHatch *brush;
        if (GdipCreateHatchBrush(style, 0xffffffff, 0xff000000, &brush) ||
            GdipFillRectangleI(graphics, (GpBrush *)brush, 0, 0, 8, 8)) return 1;
        for (int y = 0; y < 8; ++y) {
            for (int x = 0; x < 8; ++x) {
                ARGB color;
                if (GdipBitmapGetPixel(bitmap, x, y, &color) || fputc(color & 255, out) == EOF) return 1;
            }
        }
        GdipDeleteBrush((GpBrush *)brush);
    }
    GdipDeleteGraphics(graphics);
    GdipDisposeImage((GpImage *)bitmap);
    GdiplusShutdown(token);
    return fclose(out) != 0;
}
