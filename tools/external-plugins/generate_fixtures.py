"""Generate small PSD and AMV fixtures with independently known pixel values."""
from pathlib import Path
import struct as s
import zlib

out=Path(__file__).resolve().parents[2]/'tests/fixtures'
be=lambda f,*v:s.pack('>'+f,*v)
le=lambda f,*v:s.pack('<'+f,*v)
# One raw RGBA layer and matching merged RGB image, 2 by 2.
pixels=[(210,30,70,255),(20,190,90,128),(40,60,220,64),(180,120,10,255)]
channels=[bytes(p[c] for p in pixels) for c in range(4)]
extra=be('I',0)+be('I',0)+b'\x05probe\0\0'
record=be('iiiiH',0,0,2,2,4)+b''.join(be('hI',c if c<3 else -1,6) for c in range(4))
record+=b'8BIMnorm'+bytes([255,0,0,0])+be('I',len(extra))+extra
info=be('h',1)+record+b''.join(be('H',0)+c for c in channels)
if len(info)%2:info+=b'\0'
layer=be('I',len(info))+info+be('I',0)
header=b'8BPS'+be('H',1)+b'\0'*6+be('HIIHH',3,2,2,8,3)
(out/'external_probe.psd').write_bytes(header+be('II',0,0)+be('I',len(layer))+layer+be('H',0)+b''.join(channels[:3]))
# Baseline DC-only MCU. Chroma shares one predictor, Y/alpha share another.
def dc(value,chroma=False):
    if value==0:return '00' if chroma else '00'
    n=abs(value).bit_length()
    lum=['00','010','011','100','101','110','1110','11110','111110','1111110','11111110','111111110']
    chr=['00','01','10','110','1110','11110','111110','1111110','11111110','111111110','1111111110','11111111110']
    bits=value if value>0 else (1<<n)-1+value
    return (chr if chroma else lum)[n]+format(bits,f'0{n}b')
def entropy(y,alpha=None):
    # Cb=112 and Cr=160, Y is exact; q=8 gives sample=DC+128.
    bits=dc(-16,True)+'00'+dc(48,True)+'00'
    pred=0
    for value in [y-128]*4+([] if alpha is None else [alpha-128]*4):
        bits+=dc(value-pred)+'1010';pred=value
    bits+='0'*((-len(bits))%32+32)
    return int(bits,2).to_bytes(len(bits)//8,'big')
for jpeg_alpha in [False,True]:
    frames=[]
    for n,(y,a) in enumerate([(90,64),(170,192)]):
        data=entropy(y,a if jpeg_alpha else None)
        extra=b'' if jpeg_alpha else le('I',len(zlib.compress(bytes([a])*256)))
        if not jpeg_alpha:data=zlib.compress(bytes([a])*256)+data
        payload=le('IhhHH',n,0,0,16,16)+extra+data
        frames.append(b'FRAM'+le('I',len(payload))+payload)
    q=192 if jpeg_alpha else 128
    header=b'AJPM'+le('IIIIIIIHHI',0,0,40+q,0,2,30,1,16,16,1 if jpeg_alpha else 2)
    (out/('external_jpeg.amv' if jpeg_alpha else 'external_zlib.amv')).write_bytes(header+bytes([8])*q+b''.join(frames))
