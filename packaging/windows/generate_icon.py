"""Reproduce the repository-owned globe branding asset using only the stdlib.

This is an artwork maintenance tool, not a production build prerequisite.
"""
from pathlib import Path
import math
import struct


def pixel(x, y):
    # Rounded graphite tile; mint globe. All coordinates are normalized.
    if math.hypot(max(abs(x) - .70, 0), max(abs(y) - .70, 0)) > .22:
        return (0, 0, 0, 0)
    radius = math.hypot(x, y)
    globe = abs(radius - .63) < .044
    meridian = abs(math.hypot(x / .25, y / .63) - 1) < .065
    latitude = abs(abs(y) - .22) < .035 and radius < .63
    return (124, 233, 191, 255) if globe or meridian or latitude else (22, 28, 39, 255)


def frame(size):
    rows = bytearray()
    mask = bytearray()
    for y in reversed(range(size)):
        bits = bytearray((size + 31) // 32 * 4)
        for x in range(size):
            samples = [pixel(2 * (x + dx) / size - 1, 2 * (y + dy) / size - 1)
                       for dx in (.25, .75) for dy in (.25, .75)]
            rgba = [sum(p[c] for p in samples) // 4 for c in range(4)]
            rows.extend((rgba[2], rgba[1], rgba[0], rgba[3]))
            if rgba[3] == 0:
                bits[x // 8] |= 128 >> (x % 8)
        mask.extend(bits)
    return struct.pack('<IiiHHIIiiII', 40, size, size * 2, 1, 32, 0,
                       len(rows), 0, 0, 0, 0) + rows + mask


def generate():
    sizes = (16, 24, 32, 48, 64, 256)
    frames = [frame(size) for size in sizes]
    offset = 6 + 16 * len(sizes)
    output = bytearray(struct.pack('<HHH', 0, 1, len(sizes)))
    for size, data in zip(sizes, frames):
        output.extend(struct.pack('<BBBBHHII', size % 256, size % 256, 0, 0,
                                  1, 32, len(data), offset))
        offset += len(data)
    return output + b''.join(frames)


if __name__ == '__main__':
    Path(__file__).with_name('rust-browser.ico').write_bytes(generate())
