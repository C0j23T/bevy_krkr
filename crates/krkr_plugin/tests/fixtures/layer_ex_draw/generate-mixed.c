#include <windows.h>
#include <gdiplus.h>
#include <gdiplusflat.h>
#include <stdio.h>

#define CHECK(expr) do { GpStatus s=(expr); if(s!=Ok) {fprintf(stderr,"%s returned %d\n",#expr,(int)s);return 1;} } while(0)

int main(void) {
    struct GdiplusStartupInput input={1,NULL,FALSE,FALSE};
    ULONG_PTR token;
    CHECK(GdiplusStartup(&token,&input,NULL));
    HDC dc=GetDC(NULL);
    GpRectF bounds={0,0,96,80}, gradient_rect={0,0,48,16}, text_rect={4,22,86,30};
    GpMetafile *meta;
    GpGraphics *g;
    CHECK(GdipRecordMetafileFileName(L"Z:\\tmp\\layerexdraw-mixed.emf",dc,EmfTypeEmfPlusOnly,&bounds,MetafileFrameUnitPixel,NULL,&meta));
    CHECK(GdipGetImageGraphicsContext((GpImage*)meta,&g));
    CHECK(GdipSetPageUnit(g,UnitPixel));
    CHECK(GdipGraphicsClear(g,0xff202020));

    GpLineGradient *gradient;
    CHECK(GdipCreateLineBrushFromRectWithAngle(&gradient_rect,0xffff0000,0xff0000ff,0,FALSE,WrapModeTile,&gradient));
    CHECK(GdipFillRectangle(g,(GpBrush*)gradient,0,0,48,16));
    GraphicsState saved;
    CHECK(GdipSaveGraphics(g,&saved));
    CHECK(GdipSetClipRect(g,51,1,25,14,CombineModeReplace));
    CHECK(GdipSetClipRect(g,58,4,7,7,CombineModeExclude));
    GpSolidFill *solid;
    CHECK(GdipCreateSolidFill(0xff00ff00,&solid));
    CHECK(GdipFillRectangle(g,(GpBrush*)solid,48,0,45,16));
    CHECK(GdipRestoreGraphics(g,saved));

    GpFontFamily *family;
    GpFont *font;
    GpStringFormat *format;
    CHECK(GdipCreateFontFamilyFromName(L"DejaVu Sans",NULL,&family));
    CHECK(GdipCreateFont(family,14,FontStyleUnderline,UnitPixel,&font));
    CHECK(GdipStringFormatGetGenericDefault(&format));
    CHECK(GdipSetStringFormatAlign(format,StringAlignmentCenter));
    CHECK(GdipDrawString(g,L"Vello",5,font,&text_rect,format,(GpBrush*)solid));

    GpBitmap *bitmap;
    CHECK(GdipCreateBitmapFromScan0(2,2,0,PixelFormat32bppARGB,NULL,&bitmap));
    CHECK(GdipBitmapSetPixel(bitmap,0,0,0xffff00ff));
    CHECK(GdipBitmapSetPixel(bitmap,1,0,0xffffff00));
    CHECK(GdipBitmapSetPixel(bitmap,0,1,0xff0000ff));
    CHECK(GdipBitmapSetPixel(bitmap,1,1,0xff00ffff));
    CHECK(GdipDrawImageRectRect(g,(GpImage*)bitmap,4,54,20,20,0,0,2,2,UnitPixel,NULL,NULL,NULL));
    CHECK(GdipDeleteGraphics(g));
    CHECK(GdipDisposeImage((GpImage*)meta));
    GdipDisposeImage((GpImage*)bitmap);
    GdipDeleteStringFormat(format); GdipDeleteFont(font); GdipDeleteFontFamily(family);
    GdipDeleteBrush((GpBrush*)solid); GdipDeleteBrush((GpBrush*)gradient);
    ReleaseDC(NULL,dc); GdiplusShutdown(token);
    return 0;
}
