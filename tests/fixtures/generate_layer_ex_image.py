"""Generate RGBA reference pixels with the pinned upstream CPU algorithms.

CxImage notices: licenses/cximage.txt.
"""
import argparse
import hashlib
from pathlib import Path
import subprocess
import tempfile

root = Path(__file__).resolve().parents[2]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('source_directory', type=Path,
                    help='external wamsoft/layerExImage checkout at 3aff625dba5c545ba722bb43b0a5ca1ad4f4336d')
upstream = parser.parse_args().source_directory
source_path = upstream / 'LayerExImage.cpp'
if hashlib.sha256(source_path.read_bytes()).hexdigest() != '32aa8af86684f46f10d5ab39fe1a6a7a69c4556d44711dcd4c8554e6aa481faf':
    parser.error('unexpected upstream LayerExImage.cpp; use the documented fixed commit')
source = source_path.read_text()
header_path = upstream / 'LayerExImage.h'
if hashlib.sha256(header_path.read_bytes()).hexdigest() != '89614a1506090ddf86662523754d9cafc8ad154bf883a4136392544005b76a7a':
    parser.error('unexpected upstream LayerExImage.h; use the documented fixed commit')
header = header_path.read_text()
base = r'''
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <vector>
using BYTE = unsigned char;
struct RGBQUAD { BYTE rgbBlue, rgbGreen, rgbRed, rgbReserved; };
struct layerExBase {
    using DispatchT = void*;
    BYTE *_buffer; int _width, _height, _pitch;
    int _clipLeft = 0, _clipTop = 0, _clipWidth, _clipHeight;
    explicit layerExBase(DispatchT) {}
    virtual void reset() {}
    void redraw() {}
};
'''
driver = r'''
int main() {
    struct Case { int width, height, operation; bool bottomUp; };
    Case cases[] = {{16,16,0,false},{16,16,1,false},{16,16,2,false},
                    {16,16,3,false},{3,2,3,true},{16,16,4,true}};
    for (auto c : cases) {
        std::vector<BYTE> bytes(c.width*c.height*4);
        layerExImage image(nullptr);
        image._width=c.width; image._height=c.height;
        image._pitch=c.width*4*(c.bottomUp ? -1 : 1);
        image._buffer=bytes.data()+(c.bottomUp ? (c.height-1)*c.width*4 : 0);
        for (int y=0;y<c.height;y++) for (int x=0;x<c.width;x++) {
            auto p=image._buffer+y*image._pitch+x*4;
            p[2]=(17*x+31*y)%256; p[1]=(53*x+7*y)%256;
            p[0]=(11*x+67*y)%256; p[3]=(19*x+23*y)%256;
        }
        switch(c.operation) {
            case 0: image.light(30,50); break;
            case 1: image.colorize(96,230,0.6); break;
            case 2: image.modulate(-65,-45,25); break;
            case 3: image.gaussianBlur(2.0); break;
            case 4: image.colorize(0,255,1); break;
        }
        for (int y=0;y<c.height;y++) for (int x=0;x<c.width;x++) {
            auto p=image._buffer+y*image._pitch+x*4;
            BYTE rgba[]={p[2],p[1],p[0],p[3]};
            fwrite(rgba,1,4,stdout);
        }
    }
}
'''
with tempfile.TemporaryDirectory(prefix='krkr-image-reference-') as tmp:
    tmp = Path(tmp)
    cpp = tmp / 'reference.cpp'
    cpp.write_text(base + header.replace('#include "layerExBase.hpp"', '') +
                   source.replace('#include "LayerExImage.h"', '') + driver)
    subprocess.run(['c++', '-std=c++17', '-O2', '-o', str(tmp/'reference'), str(cpp)], check=True)
    pixels = subprocess.check_output([str(tmp/'reference')])
    (root/'tests/fixtures/layer-ex-image.rgba').write_bytes(pixels)
