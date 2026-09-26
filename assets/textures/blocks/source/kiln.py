"""Generate the original 32px kiln masonry/vent textures (stdlib only)."""
from pathlib import Path
import struct
import zlib


def write(name, kind):
    rows = bytearray()
    for y in range(32):
        rows.append(0)
        for x in range(32):
            mortar = y % 8 == 0 or (x + (8 if y // 8 % 2 else 0)) % 16 == 0
            noise = ((x * 17 + y * 31) % 9) - 4
            color = (55, 48, 44) if mortar else (126 + noise, 97 + noise, 76 + noise)
            if kind and 7 <= x <= 24 and 12 <= y <= 26:
                color = (29, 25, 23)
                if kind == 2 and 9 <= x <= 22 and 16 <= y <= 25:
                    color = (242, 115 + (x * 7 + y * 11) % 65, 30)
                if y in (13, 25) or x in (8, 23):
                    color = (76, 68, 61)
            rows.extend((*color, 255))
    def chunk(tag, data):
        return struct.pack('>I', len(data)) + tag + data + struct.pack('>I', zlib.crc32(tag + data))
    png = b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', 32, 32, 8, 6, 0, 0, 0))
    png += chunk(b'IDAT', zlib.compress(rows)) + chunk(b'IEND', b'')
    (Path(__file__).resolve().parent.parent / (name + '.png')).write_bytes(png)


for name, kind in [('kiln_brick', 0), ('kiln_vent', 1), ('kiln_lit', 2)]:
    write(name, kind)
