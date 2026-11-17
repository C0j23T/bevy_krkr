#include <windows.h>
#include <gdiplus.h>
#include <gdiplusflat.h>
int main(void) {
 struct GdiplusStartupInput input={1,NULL,FALSE,FALSE}; ULONG_PTR token; if(GdiplusStartup(&token,&input,NULL)) return 1;
 HDC dc=GetDC(NULL); GpMetafile *meta; GpGraphics *g; GpPath *path; GpSolidFill *brush; GpPen *pen; GpRectF bounds={0,0,64,48};
 if(GdipRecordMetafileFileName(L"Z:\\tmp\\layerexdraw-external.emf",dc,EmfTypeEmfPlusOnly,&bounds,MetafileFrameUnitPixel,NULL,&meta))return 2;
 GdipGetImageGraphicsContext((GpImage*)meta,&g); GdipSetPageUnit(g,UnitPixel); GdipSetSmoothingMode(g,SmoothingModeAntiAlias); GdipGraphicsClear(g,0xff112233);
 GdipCreatePath(FillModeAlternate,&path); GdipAddPathBezier(path,4,4,20,0,44,32,60,4); GdipAddPathLine(path,60,4,60,40); GdipAddPathLine(path,60,40,4,40); GdipClosePathFigure(path);
 GdipCreateSolidFill(0xc8e65028,&brush); GdipFillPath(g,(GpBrush*)brush,path);
 GdipCreatePen1(0xff14dc28,2.5f,UnitWorld,&pen); GdipSetPenDashStyle(pen,DashStyleDash); GdipDrawPath(g,pen,path);
 GdipDeletePen(pen); GdipDeleteBrush((GpBrush*)brush); GdipDeletePath(path); GdipDeleteGraphics(g); GdipDisposeImage((GpImage*)meta); ReleaseDC(NULL,dc); GdiplusShutdown(token); return 0;
}
