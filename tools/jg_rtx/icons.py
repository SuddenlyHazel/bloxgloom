#!/usr/bin/env python3
"""Bake bounded ItemIcon palettes/rows from the imported JG RTX albedos."""
from __future__ import annotations
import json
from pathlib import Path
import numpy as np
from PIL import Image, ImageChops, ImageDraw

ROOT = Path(__file__).resolve().parents[2]
SIZE = 24
SUPERSAMPLE = 4
SYMBOLS = '0123456789ABCDEFGHIJKLMNOPQRSTUV'


def legacy_items():
    blocks = []
    for key in ['dirt','stone','sand','snow','moss','gravel','glowstone','leaves','water']:
        blocks.append({'key':key,'kind':'cube','top':key,'side':key})
    blocks.append({'key':'grass','kind':'cube','top':'grass_top','side':'grass_side'})
    blocks.append({'key':'wood','kind':'log','top':'wood_top','side':'wood_side'})
    for key,texture in [('red_flower','flower_red'),('yellow_flower','flower_yellow'),('blue_flower','flower_blue'),('fern','fern'),('tall_grass','tall_grass'),('sapling','sapling')]:
        blocks.append({'key':key,'kind':'plant','top':texture,'side':texture})
    for key,top,side in [('kiln','kiln_brick','kiln_vent'),('hopper','hopper_top','hopper_side'),('chest','chest_top','chest_side')]:
        blocks.append({'key':key,'kind':'cube','top':top,'side':side})
    return blocks


def load_texture(paths, key):
    with Image.open(paths[key]) as im:
        return im.convert('RGBA')


def face(image, origin, u, v, brightness):
    scale = SUPERSAMPLE
    origin = np.array(origin,dtype=float)*scale
    u = np.array(u,dtype=float)*scale
    v = np.array(v,dtype=float)*scale
    inverse = np.linalg.inv(np.column_stack([u,v]))
    affine = (image.width*inverse[0,0],image.width*inverse[0,1],-image.width*float(inverse[0]@origin),
              image.height*inverse[1,0],image.height*inverse[1,1],-image.height*float(inverse[1]@origin))
    projected = image.transform((SIZE*scale,SIZE*scale),Image.Transform.AFFINE,affine,Image.Resampling.BICUBIC)
    data = np.array(projected)
    data[:,:,:3] = np.rint(data[:,:,:3].astype(float)*brightness).astype(np.uint8)
    projected = Image.fromarray(data)
    mask = Image.new('L',projected.size)
    ImageDraw.Draw(mask).polygon([tuple(origin),tuple(origin+u),tuple(origin+u+v),tuple(origin+v)],fill=255)
    projected.putalpha(ImageChops.multiply(projected.getchannel('A'),mask))
    return projected


def cube_icon(paths, block):
    side = load_texture(paths,block['side'])
    top = load_texture(paths,block['top'])
    canvas = Image.new('RGBA',(SIZE*SUPERSAMPLE,SIZE*SUPERSAMPLE))
    canvas.alpha_composite(face(side,(2,6),(10,5),(0,12),.85))
    canvas.alpha_composite(face(side,(12,11),(10,-5),(0,12),.66))
    canvas.alpha_composite(face(top,(12,1),(10,5),(-10,5),1.))
    return canvas.resize((SIZE,SIZE),Image.Resampling.LANCZOS)


def sprite_icon(paths, block):
    lower = load_texture(paths,block['side'])
    if block['kind'] == 'tall_plant':
        upper = load_texture(paths,block['top_half']).resize(lower.size,Image.Resampling.LANCZOS)
        combined = Image.new('RGBA',(lower.width,lower.height*2))
        combined.alpha_composite(upper)
        combined.alpha_composite(lower,(0,lower.height))
        lower = combined
    bounds = lower.getchannel('A').getbbox()
    if bounds is None:
        raise ValueError(f'Invisible source sprite: {block["key"]}')
    sprite = lower.crop(bounds)
    sprite.thumbnail((SIZE-2,SIZE-2),Image.Resampling.LANCZOS)
    canvas = Image.new('RGBA',(SIZE,SIZE))
    canvas.alpha_composite(sprite,((SIZE-sprite.width)//2,(SIZE-sprite.height)//2))
    return canvas


def quantized_icon(block, canvas):
    # HUD icons use crisp transparent pixels, while preserving the source shape.
    data = np.array(canvas)
    # Very thin sprites such as lichen lose coverage when reduced to HUD size.
    # Lower their cutoff relative to maximum coverage so they remain visible.
    cutoff = min(96,max(1,int(data[:,:,3].max())//2))
    data[:,:,3] = np.where(data[:,:,3] >= cutoff,255,0)
    data[data[:,:,3] == 0] = 0
    quantized = Image.fromarray(data).quantize(colors=32,method=Image.Quantize.FASTOCTREE,dither=Image.Dither.NONE).convert('RGBA')
    pixels = np.array(quantized)
    colors = sorted({tuple(int(c) for c in pixel) for row in pixels for pixel in row if pixel[3] >= 128})
    if not colors or len(colors)>len(SYMBOLS):
        raise ValueError(f'Invalid icon palette for {block["key"]}: {len(colors)} colors')
    symbols = {color:SYMBOLS[index] for index,color in enumerate(colors)}
    rows = [''.join(symbols[tuple(int(c) for c in pixel)] if pixel[3]>=128 else '.' for pixel in row) for row in pixels]
    return {'key':block['key'],'rows':rows,'palette':[[ord(symbols[color]),[round(c/255,8) for c in color]] for color in colors]}


def generate():
    manifest=json.loads((ROOT/'assets/jg-rtx/catalog.json').read_text())
    paths={t['key']:ROOT/t['path'] for t in manifest['textures']}
    for folder in ['blocks','foliage','items']:
        for path in (ROOT/'assets/textures'/folder).glob('*.png'):
            paths.setdefault(path.stem,path)
    icons=[]
    for block in manifest['blocks']+legacy_items():
        canvas=sprite_icon(paths,block) if block['kind'] in ['plant','tall_plant'] else cube_icon(paths,block)
        icons.append(quantized_icon(block,canvas))
    keys=[icon['key'] for icon in icons]
    if len(set(keys)) != len(keys): raise ValueError('Duplicate icon item key')
    target=ROOT/'assets/jg-rtx/icons.json'
    pending=target.with_suffix('.json.tmp')
    pending.write_text(json.dumps({'icons':icons},separators=(',',':'))+'\n')
    pending.replace(target)
    print(f'Generated {len(icons)} source-art inventory icons, {SIZE}x{SIZE}, <=32 palette colors.')

if __name__ == '__main__': generate()
