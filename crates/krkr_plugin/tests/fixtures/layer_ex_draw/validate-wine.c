#include <windows.h>
#include <gdiplus.h>
#include <gdiplusflat.h>
#include <stdio.h>
static int records=0,paths=0,pens=0,brushes=0,draws=0;
static BOOL CALLBACK visit(EmfPlusRecordType type,UINT flags,UINT size,const BYTE *data,void *context) {
 GpMetafile *meta=context; records++; if(type==EmfPlusRecordTypeObject) { if((flags>>8&127)==3)paths++;if((flags>>8&127)==2)pens++;if((flags>>8&127)==1)brushes++; }
 if(type==EmfPlusRecordTypeDrawPath||type==EmfPlusRecordTypeFillPath)draws++;
 Status s=GdipPlayMetafileRecord(meta,type,flags,size,data); if(s)printf("play failed type %x status %d\n",type,s); return TRUE;
}
int main(void) {
 struct GdiplusStartupInput input={1,NULL,FALSE,FALSE}; ULONG_PTR token; GdiplusStartup(&token,&input,NULL);
 GpMetafile *meta; Status s=GdipCreateMetafileFromFile(L"Z:\\tmp\\layerexdraw-reencoded.emf",&meta); if(s){printf("open %d\n",s);return 1;}
 GpBitmap *bitmap; GpGraphics *graphics; GdipCreateBitmapFromScan0(64,48,0,PixelFormat32bppARGB,NULL,&bitmap);GdipGetImageGraphicsContext((GpImage*)bitmap,&graphics);
 GpRectF dest={0,0,64,48};s=GdipEnumerateMetafileDestRect(graphics,meta,&dest,visit,meta,NULL);printf("enumerate %d records %d paths %d pens %d brushes %d draws %d\n",s,records,paths,pens,brushes,draws);
 ARGB color;GdipBitmapGetPixel(bitmap,20,30,&color);printf("sample pixel %08x\n",color);
 GdipDeleteGraphics(graphics);GdipDisposeImage((GpImage*)bitmap);GdipDisposeImage((GpImage*)meta);GdiplusShutdown(token);return s||paths!=2||pens!=1||brushes!=1||draws!=2;
}
