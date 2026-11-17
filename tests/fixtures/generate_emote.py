"""Generate a small, original PSB fixture for Emote control and shape tests."""
from pathlib import Path
import struct


def array(values):
    return b'\x10' + struct.pack('<I', len(values)) + b'\x10' + b''.join(struct.pack('<I', v) for v in values)


def frame(time, value):
    return {'time': time, 'type': 0, 'content': {'value': value, 'easing': 0}}


def timeline(label, diff, end):
    return {'label': label, 'diff': diff, 'lastTime': 19, 'loopBegin': -1, 'loopEnd': -1,
            'variableList': [{'label': 'pose', 'frameList': [frame(0, end), {'time': 20, 'type': 0}]}]}


root = {
    'spec': 'krkr',
    'metadata': {'base': {'chara': 'test', 'motion': 'idle'}, 'scale': 1,
                 'variableList': [{'label': 'pose'}],
                 'timelineControl': [timeline('main', 0, 100), timeline('diff', 1, 20)]},
    'object': {'test': {'type': 0, 'motion': {'idle': {
        'lastTime': 1000, 'loopTime': 0,
        'layer': [{'type': 0, 'label': 'anchor',
                   'frameList': [{'time': 0, 'type': 0, 'content': {'src': 'shape/rect', 'coord': [0, 0, 0], 'zx': 1, 'zy': 1}}],
                   'children': []}],
        'priority': [{'content': [0]}]}}}},
    'screenSize': {'originX': 32, 'originY': 32, 'width': 64, 'height': 64},
    'source': {},
}
motion = root['object']['test']['motion']['idle']
motion['parameter'] = [{'id': 'pose', 'rangeBegin': 0, 'rangeEnd': 100, 'division': 100}]
motion['layer'].append({'type': 0, 'label': 'pose-shape', 'parameterize': 0, 'children': [],
                        'frameList': [{'time': time, 'type': 0,
                                       'content': {'src': 'shape/rect', 'coord': [x, 0, 0], 'zx': 1, 'zy': 1}}
                                      for time, x in [(0, 0), (100, 40)]]})
motion['priority'] = [{'content': [0, 1]}]
names, strings = [], []


def collect(value):
    if isinstance(value, dict):
        for key, item in value.items():
            if key not in names:
                names.append(key)
            collect(item)
    elif isinstance(value, list):
        for item in value:
            collect(item)
    elif isinstance(value, str) and value not in strings:
        strings.append(value)


def encode(value):
    if isinstance(value, dict):
        items = sorted(value.items(), key=lambda pair: names.index(pair[0]))
        data, offsets = b'', []
        for _, item in items:
            offsets.append(len(data))
            data += encode(item)
        return b'\x21' + array([names.index(key) for key, _ in items]) + array(offsets) + data
    if isinstance(value, list):
        data, offsets = b'', []
        for item in value:
            offsets.append(len(data))
            data += encode(item)
        return b'\x20' + array(offsets) + data
    if isinstance(value, str):
        return b'\x18' + struct.pack('<I', strings.index(value))
    return b'\x08' + struct.pack('<i', value)


collect(root)
trie = {}
for name in names:
    node = trie
    for ch in name.encode():
        node = node.setdefault(ch, {})
parents, bases, occupied = [0], [0], {0}
queue = [(0, trie)]
for parent, children in queue:
    base = 1
    while any(base + ch in occupied for ch in children):
        base += 1
    bases[parent] = base
    for ch, grandchildren in children.items():
        node = base + ch
        occupied.add(node)
        if node >= len(parents):
            parents.extend([0] * (node + 1 - len(parents)))
            bases.extend([0] * (node + 1 - len(bases)))
        parents[node] = parent
        queue.append((node, grandchildren))
leaves = []
for name in names:
    node = 0
    for ch in name.encode():
        node = bases[node] + ch
    leaves.append(node)
slots = list(range(len(parents), len(parents) + len(leaves)))
parents.extend(leaves)
bases.extend([0] * len(leaves))
data = bytearray(44)
positions = [len(data), len(data)]
data += array(bases) + array(parents) + array(slots)
positions.append(len(data))
string_data, offsets = b'', []
for value in strings:
    offsets.append(len(string_data))
    string_data += value.encode() + b'\0'
data += array(offsets)
positions.append(len(data))
data += string_data
positions.append(len(data))
data += array([])
positions.append(len(data))
data += array([])
positions.extend([len(data), len(data)])
data += encode(root)
data[:8] = b'PSB\0\x03\0\0\0'
struct.pack_into('<8I', data, 8, *positions)
Path(__file__).with_name('emote-controls.psb').write_bytes(data)
