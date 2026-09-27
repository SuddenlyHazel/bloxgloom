"""Original riveted metal hopper pixel art; standard-library PNG writer."""
from pathlib import Path
import struct
import zlib

for name, top in [('hopper_side', False), ('hopper_top', True)]:
    rows = bytearray()
    for y in range(32):
        rows.append(0)
        for x in range(32):
            shade = 90 + (x * 7 + y * 3) % 7
            if min(x, y, 31-x, 31-y) < 3:
                shade = 135
            elif top:
                shade = 32 + min(x, y, 31-x, 31-y) * 2
            elif (13 <= x <= 18 and 8 <= y <= 19) or (19 <= y <= 24 and abs(x-15) <= 24-y):
                shade = 190
            rows.extend((shade, shade + 5, shade + 8, 255))
    def chunk(tag, data):
        return struct.pack('>I', len(data)) + tag + data + struct.pack('>I', zlib.crc32(tag + data))
    png = b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', 32, 32, 8, 6, 0, 0, 0))
    png += chunk(b'IDAT', zlib.compress(rows)) + chunk(b'IEND', b'')
    (Path(__file__).resolve().parent.parent / (name + '.png')).write_bytes(png)
