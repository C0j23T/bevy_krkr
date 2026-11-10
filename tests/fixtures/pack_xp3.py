"""Pack an unencrypted NVL export for the directory/XP3 acceptance comparison."""
from pathlib import Path
import struct
import sys
import zlib


def chunk(name, data):
    return name + struct.pack('<Q', len(data)) + data


def pack(source, target):
    index = bytearray()
    with target.open('wb') as out:
        out.write(b'XP3\r\n \n\x1a\x8b\x67\x01' + bytes(8))
        for path in sorted(source.rglob('*')):
            if not path.is_file():
                continue
            data = path.read_bytes()
            name = path.relative_to(source).as_posix().encode('utf-16-le')
            compressed = zlib.compress(data)
            use_compressed = len(compressed) < len(data)
            stored = compressed if use_compressed else data
            offset = out.tell()
            out.write(stored)
            info = struct.pack('<IQQH', 0, len(data), len(stored), len(name)//2) + name
            segment = struct.pack('<IQQQ', int(use_compressed), offset, len(data), len(stored))
            index += chunk(b'File', chunk(b'info', info) + chunk(b'segm', segment) +
                           chunk(b'adlr', struct.pack('<I', zlib.adler32(data))))
        offset = out.tell()
        compressed = zlib.compress(index)
        out.write(bytes([1]) + struct.pack('<QQ', len(compressed), len(index)) + compressed)
        out.seek(11)
        out.write(struct.pack('<Q', offset))


if __name__ == '__main__':
    pack(Path(sys.argv[1]), Path(sys.argv[2]))
