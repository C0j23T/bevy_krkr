"""Generate sampled RGB references from an external extrans source checkout.

Usage: python3 tests/fixtures/generate_extrans_geometry.py /path/to/extrans
The reference compiles the upstream geometry and uses its fixed-point sampling.
"""
import pathlib
import re
import subprocess
import sys
import tempfile

source = pathlib.Path(sys.argv[1])

def extract(text, signature):
    start = text.index(signature)
    brace = text.index('{', start)
    end, depth = brace + 1, 1
    while depth:
        depth += (text[end] == '{') - (text[end] == '}')
        end += 1
    return text[start:end]

base_header = (source / 'rotatebase.h').read_text()
base_header = re.sub(r'#include[^\n]*', '', base_header)
base_header = base_header.replace('protected:', 'public:')
base = (source / 'rotatebase.cpp').read_text()
base = re.sub(r'#include[^\n]*', '', base)
base = base.replace(extract(base, 'tjs_error TJS_INTF_METHOD tTVPBaseRotateTransHandler::Process('), '')
rotate = (source / 'rotatetrans.cpp').read_text()
classes = extract(rotate, 'class tTVPRotateZoomTransHandler ') + ';\n'
classes += extract(rotate, 'class tTVPRotateSwapTransHandler ') + ';\n'
preamble = '''#include <cmath>
#include <cstdio>
#include <cstdint>
#include <cstring>
#include <memory>
using tjs_int=int; using tjs_uint=unsigned; using tjs_uint32=uint32_t;
using tjs_uint64=uint64_t; using tjs_error=int;
#define TJS_INTF_METHOD
#define TJS_S_OK 0
#define TJS_S_TRUE 1
#define TJS_S_FALSE 2
struct iTVPSimpleOptionProvider {}; struct iTVPDivisibleTransHandler {}; struct iTVPScanLineProvider {}; struct tTVPDivisibleData {};
'''
common = re.sub(r'#include[^\n]*', '', (source / 'common.h').read_text())
table_header = (source / 'turntrans_table.h').read_text()
table = re.sub(r'#include[^\n]*', '', (source / 'turntrans_table.cpp').read_text())
main = r'''
uint32_t pixel(int x,int y,bool second) {
    unsigned r=second?(x*11+y*3)%256:(x*5+y*7)%256;
    unsigned g=second?(x*2+y*13)%256:(x*17+y*2)%256;
    unsigned b=second?(x*19+y*5)%256:(x*3+y*23)%256;
    return 0xff000000u|(r<<16)|(g<<8)|b;
}
int main() {
    constexpr int w=130,h=98;
    const int times[]={1,25,50,75,95,99};
    const int gloss[]={0,0,0,0,16,48,80,128,192,128,80,48,16};
    for(int effect=0;effect<6;effect++) for(int direction=0;direction<2;direction++) {
        std::unique_ptr<tTVPBaseRotateTransHandler> handler;
        if(effect==1) handler.reset(new tTVPRotateZoomTransHandler(100,w,h,.35,1,-1.5,1.3,.7,41,29,true));
        if(effect==2) handler.reset(new tTVPRotateZoomTransHandler(100,w,h,1,0,1.7,-1.1,1.4,41,29,false));
        if(effect==4) handler.reset(new tTVPRotateZoomTransHandler(100,w,h,1.6,1,.5,1.5,1.4,w/2,h/2,true));
        if(effect==5) handler.reset(new tTVPRotateZoomTransHandler(100,w,h,0,1,1,2,1,w/2,h/2,true));
        if(effect==3) handler.reset(new tTVPRotateSwapTransHandler(100,w,h,0xff172b49,1.3));
        if(handler) handler->StartProcess(0);
        for(int time:times) {
            if(handler) handler->StartProcess(time);
            for(int y=1;y<h;y+=3) for(int x=1;x<w;x+=3) {
                uint32_t color=(effect!=0 && effect!=3)?0:0xff172b49;
                if(effect==0) {
                    int phase=time*(64+((w+63)/64+(h+63)/64)*2)/100-((h+63)/64)*2-(x/64-y/64)*2;
                    if(phase<0)phase=0;if(phase>63)phase=63;
                    if(phase==0||phase==63) color=pixel(x,y,(phase==63)^bool(direction));
                    else {
                        auto &row=TurnTransParams[phase][y%64];
                        if(x%64>=row.start && x%64<row.start+row.len) {
                            int xx=x/64*64+((row.sx+(x%64-row.start)*row.stepx)>>16);
                            int yy=y/64*64+((row.sy+(x%64-row.start)*row.stepy)>>16);
                            if(xx>=0 && xx<w && yy>=0 && yy<h) {
                                color=pixel(xx,yy,(phase>=32)^bool(direction));
                                if(phase<13 && gloss[phase]) color=Blend(color,0xffffff,gloss[phase]);
                            }
                        }
                    }
                } else {
                    auto &row=handler->DrawData[y];
                    for(int i=0;i<row.count;i++) {
                        auto &region=row.region[i];
                        if(x<region.left || x>=region.right || region.type==0)continue;
                        auto &line=region.type==1?row.src1:row.src2;
                        int xx=(line.sx+(x-line.start)*line.stepx)>>16;
                        int yy=(line.sy+(x-line.start)*line.stepy)>>16;
                        if(xx>=0 && xx<w && yy>=0 && yy<h)color=pixel(xx,yy,(region.type==2)^bool(direction));
                    }
                }
                unsigned char rgb[]={static_cast<unsigned char>(color>>16),static_cast<unsigned char>(color>>8),static_cast<unsigned char>(color)};
                fwrite(rgb,1,3,stdout);
            }
        }
    }
}
'''
with tempfile.TemporaryDirectory() as tmp:
    cpp = pathlib.Path(tmp) / 'reference.cpp'
    cpp.write_text(preamble + common + base_header + base + classes + table_header + table + main)
    binary = pathlib.Path(tmp) / 'reference'
    subprocess.run(['c++', '-std=c++17', '-O2', str(cpp), '-o', str(binary)], check=True)
    output = pathlib.Path(__file__).with_name('extrans_geometry.rgb')
    output.write_bytes(subprocess.check_output([str(binary)]))
    print(output)

import struct
for second in [False, True]:
    w,h=130,98
    stride=(w*3+3)//4*4
    pixels=bytearray()
    for y in reversed(range(h)):
        for x in range(w):
            r=(x*11+y*3)%256 if second else (x*5+y*7)%256
            g=(x*2+y*13)%256 if second else (x*17+y*2)%256
            b=(x*19+y*5)%256 if second else (x*3+y*23)%256
            pixels.extend([b,g,r])
        pixels.extend(bytes(stride-w*3))
    header=b'BM'+struct.pack('<IHHI',54+len(pixels),0,0,54)
    header+=struct.pack('<IiiHHIIiiII',40,w,h,1,24,0,len(pixels),0,0,0,0)
    pathlib.Path(__file__).with_name('extrans_'+('b' if second else 'a')+'.bmp').write_bytes(header+pixels)
