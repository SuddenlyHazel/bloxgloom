"""Original wooden chest pixel art, generated with Python's standard library."""
from pathlib import Path
import struct
import zlib

for name, top in [('chest_side', False), ('chest_top', True)]:
    rows = bytearray()
    for y in range(32):
        rows.append(0)
        for x in range(32):
            grain = (x*13+y*3) % 11 - 5
            color = (137+grain, 87+grain, 42+grain)
            if y % 8 == 0:
                color = (75, 43, 24)
            if x < 3 or x > 28 or y < 3 or y > 28 or (not top and y in (10,11,12)):
                color = (52, 43, 35)
            if not top and 13 <= x <= 18 and 9 <= y <= 18:
                color = (210, 165, 70) if x in (13,18) or y in (9,18) else (156,113,42)
            if not top and 15 <= x <= 16 and 12 <= y <= 15:
                color = (37, 29, 20)
            rows.extend((*color,255))
    def chunk(tag, data):
        return struct.pack('>I',len(data))+tag+data+struct.pack('>I',zlib.crc32(tag+data))
    png = b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',32,32,8,6,0,0,0))
    png += chunk(b'IDAT',zlib.compress(rows))+chunk(b'IEND',b'')
    (Path(__file__).resolve().parent.parent/(name+'.png')).write_bytes(png)
