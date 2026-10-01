#!/usr/bin/env python3
"""Generate valid admission fixtures separately from farming gameplay."""
import argparse
from pathlib import Path
import random
import struct
import zlib

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('output', type=Path)
parser.add_argument('--packages',type=int,default=4)
parser.add_argument('--modules',type=int,default=256)
parser.add_argument('--assets',type=int,default=256)
parser.add_argument('--image-size',type=int,default=64)
args = parser.parse_args()
assert args.packages > 0 and args.modules > 0 and args.assets >= 0 and args.image_size > 0
args.output.mkdir(parents=True,exist_ok=False)

def chunk(kind,data):
    return struct.pack('>I',len(data))+kind+data+struct.pack('>I',zlib.crc32(kind+data))

rng = random.Random(81345)
size = args.image_size
pixels = b''.join(b'\0'+bytes(rng.randrange(256) for _ in range(size*4)) for _ in range(size))
png = b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',size,size,8,6,0,0,0))+chunk(b'IDAT',zlib.compress(pixels))+chunk(b'IEND',b'')
for number in range(args.packages):
    name = f'pressure{number}'
    root = args.output/name
    (root/'server').mkdir(parents=True)
    (root/'client').mkdir()
    (root/'assets'/'textures').mkdir(parents=True)
    manifest = [f'format 2\npackage {name}\nversion 1.0.0\nentry main', 'module server main server/main.luau']
    (root/'server/main.luau').write_text('return function(_) end\n')
    for index in range(args.modules-1):
        key = f'view{index:03}'
        (root/'client'/f'{key}.luau').write_text('return function(_) return {} end\n')
        manifest.append(f'module client {key} client/{key}.luau')
    for index in range(args.assets):
        key = f'tile{index:03}'
        (root/'assets'/'textures'/f'{key}.png').write_bytes(png)
        manifest.append(f'asset texture {key} assets/textures/{key}.png')
    (root/'package.txt').write_text('\n'.join(manifest)+'\n')
files = list(args.output.rglob('*'))
print(f'{args.packages*args.modules} modules, {args.packages*args.assets} valid RGBA assets; {sum(p.stat().st_size for p in files if p.is_file()):,} file bytes')
