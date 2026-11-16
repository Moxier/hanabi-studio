#!/usr/bin/env python3
"""Write a tiny PMX 2.0 test model (a skinned two-part column with a morph, physics and a joint)
built from the published file layout. Used to test the loader; contains no third-party data."""
import struct, sys

def text(s):
    b = s.encode('utf-16-le')
    return struct.pack('<i', len(b)) + b

out = bytearray()
out += b'PMX ' + struct.pack('<f', 2.0)
out += bytes([8, 0, 0, 4, 4, 4, 4, 4, 4])  # UTF-16, 0 extra uv, all index sizes 4
out += text('テストモデル') + text('Test model') + text('generated for loader tests') + text('')

# geometry: a box from y=0..10 split in two segments, 4 verts per ring, 3 rings
verts = []
for ring, y in enumerate([0.0, 5.0, 10.0]):
    for (x, z) in [(-1, -1), (1, -1), (1, 1), (-1, 1)]:
        w = 1.0 - ring / 2.0
        verts.append(((x, y, z), (x * 0.7, 0, z * 0.7), (0.5, ring / 2.0), w))
out += struct.pack('<i', len(verts))
for (p, n, uv, w) in verts:
    out += struct.pack('<3f3f2f', *p, *n, *uv)
    out += bytes([1])  # BDEF2
    out += struct.pack('<iif', 0, 1, w)
    out += struct.pack('<f', 1.0)
faces = []
for ring in range(2):
    for k in range(4):
        a = ring * 4 + k
        b = ring * 4 + (k + 1) % 4
        c = a + 4
        d = b + 4
        faces += [a, c, b, b, c, d]
out += struct.pack('<i', len(faces))
for f in faces:
    out += struct.pack('<i', f)
out += struct.pack('<i', 0)  # textures
# one material
out += struct.pack('<i', 1)
out += text('body') + text('body')
out += struct.pack('<4f3ff3f', 0.8, 0.5, 0.6, 1.0, 0.2, 0.2, 0.2, 10.0, 0.4, 0.25, 0.3)
out += bytes([0x1E])  # ground shadow, cast, receive, edge
out += struct.pack('<4ff', 0, 0, 0, 1, 1.0)
out += struct.pack('<ii', -1, -1) + bytes([0, 1, 1]) + text('') + struct.pack('<i', len(faces))
# bones: center, upper, and an IK-free tail
bones = [('センター', (0, 0, 0), -1, 0x001E), ('上半身', (0, 5, 0), 0, 0x001A)]
out += struct.pack('<i', len(bones))
for name, p, parent, flags in bones:
    out += text(name) + text('') + struct.pack('<3fii', *p, parent, 0) + struct.pack('<H', flags)
    out += struct.pack('<3f', 0, 2, 0)  # tail offset
# morph: vertex morph widening the top ring
out += struct.pack('<i', 1)
out += text('あ') + text('a') + bytes([3, 1]) + struct.pack('<i', 4)
for i in range(8, 12):
    out += struct.pack('<i3f', i, 0.5, 0.5, 0)
# display frames
out += struct.pack('<i', 2)
out += text('Root') + text('Root') + bytes([1]) + struct.pack('<i', 1) + bytes([0]) + struct.pack('<i', 0)
out += text('表情') + text('Exp') + bytes([1]) + struct.pack('<i', 1) + bytes([1]) + struct.pack('<i', 0)
# rigid bodies: one following the centre bone, one simulated on the upper bone
out += struct.pack('<i', 2)
for name, bone, pos, mode in [('base', 0, (0, 2.5, 0), 0), ('top', 1, (0, 7.5, 0), 1)]:
    out += text(name) + text('') + struct.pack('<i', bone) + bytes([0]) + struct.pack('<H', 0xFFFF) + bytes([2])
    out += struct.pack('<3f3f3f', 1.0, 4.0, 0, *pos, 0, 0, 0)
    out += struct.pack('<5f', 1.0, 0.5, 0.5, 0.0, 0.5) + bytes([mode])
# joint
out += struct.pack('<i', 1)
out += text('j') + text('') + bytes([0]) + struct.pack('<ii', 0, 1)
out += struct.pack('<3f3f', 0, 5, 0, 0, 0, 0)
out += struct.pack('<3f3f3f3f3f3f', 0, 0, 0, 0, 0, 0, -0.3, -0.3, -0.3, 0.3, 0.3, 0.3, 0, 0, 0, 10, 10, 10)
open(sys.argv[1] if len(sys.argv) > 1 else 'test.pmx', 'wb').write(out)
print('wrote', len(out), 'bytes')
