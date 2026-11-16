#!/usr/bin/env python3
import struct
import zlib
from pathlib import Path


def chunks(data):
    position = 0
    while position < len(data):
        name = data[position:position + 4]
        size = struct.unpack_from('<Q', data, position + 4)[0]
        yield name, data[position + 12:position + 12 + size]
        position += 12 + size


def extract(archive, member, output):
    with archive.open('rb') as source:
        if source.read(11) != b'XP3\r\n \n\x1a\x8b\x67\x01':
            raise ValueError('invalid XP3 signature')
        offset = struct.unpack('<Q', source.read(8))[0]
        source.seek(offset)
        flags = source.read(1)[0]
        if flags & 0x80:
            raise ValueError('continued XP3 index is unsupported')
        if flags & 1:
            packed, original = struct.unpack('<QQ', source.read(16))
            index = zlib.decompress(source.read(packed))
            assert len(index) == original
        else:
            size = struct.unpack('<Q', source.read(8))[0]
            index = source.read(size)
        for kind, entry in chunks(index):
            if kind != b'File':
                continue
            fields = dict(chunks(entry))
            info = fields[b'info']
            name_length = struct.unpack_from('<H', info, 20)[0]
            name = info[22:22 + name_length * 2].decode('utf-16-le')
            if name.replace('\\', '/').lower() != member.lower():
                continue
            result = bytearray()
            for position in range(0, len(fields[b'segm']), 28):
                flags, offset, original, packed = struct.unpack_from('<IQQQ', fields[b'segm'], position)
                source.seek(offset)
                data = source.read(packed)
                data = zlib.decompress(data) if flags & 1 else data
                assert len(data) == original
                result.extend(data)
            assert len(result) == struct.unpack_from('<Q', info, 4)[0]
            output.parent.mkdir(parents=True, exist_ok=True)
            output.write_bytes(result)
            print(f'{name} -> {output} ({len(result)} bytes)')
            return
    raise ValueError(f'missing XP3 member: {member}')


if __name__ == '__main__':
    extract(Path('test_games/Fate hollow ataraxia/data.xp3'), 'video/fd_op01.mpg',
            Path('target/android-video-test/media/fd_op01.mpg'))
