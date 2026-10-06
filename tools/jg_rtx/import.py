#!/usr/bin/env python3
"""Reproducibly import selected JG RTX materials (requires Pillow and NumPy)."""
from __future__ import annotations
import argparse
import json
import shutil
import subprocess
import sys
from pathlib import Path
import numpy as np
from PIL import Image
import pbr
import color as albedo
import art

ROOT = Path(__file__).resolve().parents[2]
MAX_SIZE = 256
TEXTURES = {}
BLOCKS = []
PROVENANCE = []
BLOCK_IDS = {}


def save_png(image, path):
    """Don't expose a truncated include_bytes! asset to concurrent builds."""
    pending = path.with_suffix('.png.tmp')
    image.save(pending, format='PNG', optimize=True)
    pending.replace(path)


def frame(path):
    im = Image.open(path).convert('RGBA')
    if im.height > im.width and im.height % im.width == 0:
        im = im.crop((0, 0, im.width, im.width))
    if max(im.size) > MAX_SIZE:
        scale = MAX_SIZE / max(im.size)
        im = albedo.resize(im, (round(im.width * scale), round(im.height * scale)))
    return im


def find(stem, area='block'):
    p = JAVA / area / (stem + '.png')
    if p.exists() and Image.open(p).convert('RGBA').getchannel('A').getextrema()[1] > 0:
        return p, 'java'
    aliases = {'pale_oak_log':'pale_oak_log_side', 'stripped_pale_oak_log':'stripped_pale_oak_log_side',
               'dry_grass':'short_dry_grass', 'dead_bush':'deadbush'}
    stem = aliases.get(stem, stem)
    for ext in ['.png', '.tga']:
        p = BEDROCK / (stem + ext)
        if p.exists() and Image.open(p).convert('RGBA').getchannel('A').getextrema()[1] > 0:
            return p, 'bedrock'
    raise FileNotFoundError(stem)


def import_texture(stem, *, key=None, folder='blocks', cutout=False, tint=None, area='block'):
    key = key or 'jg_' + stem
    if key in TEXTURES:
        return key
    path, edition = find(stem, area)
    color = frame(path)
    if color.getchannel('A').getextrema()[1] == 0:
        raise ValueError(f'Imported frame is wholly transparent: {path}')
    if tint:
        arr = np.array(color)
        arr[:,:,:3] = np.rint(arr[:,:,:3].astype(float) * np.array(tint) / 255).astype(np.uint8)
        color = Image.fromarray(arr)
    # A block face must cover the entire cube; alpha is meaningful only for foliage/items.
    if not cutout:
        color.putalpha(255)
    color, diffuse_art = art.bark(color, key)
    dest = ROOT / 'assets/textures' / folder / (key + '.png')
    dest.parent.mkdir(parents=True, exist_ok=True)
    save_png(color, dest)
    companions = []
    n_path = path.with_name(path.stem + ('_n.png' if edition == 'java' else '_normal.png'))
    s_path = path.with_name(path.stem + ('_s.png' if edition == 'java' else '_mer.png'))
    if edition == 'bedrock' and not s_path.exists():
        for extension in ['.png', '.tga']:
            candidate = path.with_name(path.stem + '_mers' + extension)
            if candidate.exists():
                s_path = candidate
                break
    maps = {}
    for suffix in ['n', 's']:
        src = n_path if suffix == 'n' else s_path
        if edition == 'java' and src.exists():
            im = pbr.data_image(src, color.size)
        elif suffix == 'n':
            im = Image.new('RGBA', color.size, (128, 128, 255, 255))
            if src.exists():
                a = np.array(pbr.data_image(src, color.size))
                a[:,:,2] = 255  # No source AO: full ambient visibility.
                a[:,:,3] = 255  # Bedrock XY already uses renderer DirectX convention.
                h = path.with_name(path.stem + '_heightmap.png')
                if h.exists():
                    a[:,:,3] = np.array(pbr.data_image(h, color.size))[:,:,0]
                im = Image.fromarray(a)
        else:
            im = Image.new('RGBA', color.size, (0, 10, 0, 255))
            if src.exists():
                mer = np.array(pbr.data_image(src, color.size)).astype(float) / 255
                a = np.zeros((*mer.shape[:2], 4), dtype=np.uint8)
                a[:,:,0] = np.rint((1 - mer[:,:,2]) * 255).astype(np.uint8)
                a[:,:,1] = np.where(mer[:,:,0] >= .5, 255, 10)
                if '_mers' in src.stem:
                    a[:,:,2] = np.where(mer[:,:,3] > 0, 65 + np.rint(mer[:,:,3]*190), 0).astype(np.uint8)
                # 255 means no emission in labPBR; 0..254 is emissive intensity.
                a[:,:,3] = np.where(mer[:,:,1] > 0, np.rint(mer[:,:,1]*254), 255).astype(np.uint8)
                im = Image.fromarray(a)
        maps[suffix] = im
    maps['n'], maps['s'], conversion = pbr.repair_companions(
        BEDROCK, path.stem, maps['n'], maps['s'],
        n_path if edition == 'java' and n_path.exists() else None, color.size)
    maps['s'], material_art = art.gloss(maps['s'], stem, cutout)
    for suffix, im in maps.items():
        companion = dest.with_stem(dest.stem + '_' + suffix)
        save_png(im, companion)
        companions.append({'key':key+'_'+suffix, 'path':str(companion.relative_to(ROOT)),
                           'alpha_cutout':False, 'stitch_edges':False, 'stitch_vertical':False,
                           'emission_strength':0., 'foliage':{'wrap':0.,'transmission':0.}})
    TEXTURES[key] = {'key':key, 'path':str(dest.relative_to(ROOT)), 'alpha_cutout':cutout,
                     'stitch_edges':False, 'stitch_vertical':False, 'emission_strength':0.,
                     'foliage':{'wrap':.35 if cutout and art.botanical(stem) else 0.,'transmission':.28 if cutout and art.botanical(stem) else 0.},
                     '_companions':companions}
    PROVENANCE.append({'destination':str(dest.relative_to(ROOT)), 'source':str(path.relative_to(SOURCE)),
                       'normal_source':str(n_path.relative_to(SOURCE)) if n_path.exists() else None,
                       'material_source':str(s_path.relative_to(SOURCE)) if s_path.exists() else None,
                       'edition':edition, 'tint':tint})
    record = PROVENANCE[-1]
    if diffuse_art or material_art:
        record['art_curation'] = {kind: value for kind, value in
                                  [('albedo', diffuse_art), ('specular', material_art)] if value}
    record['smoothness_source'] = record['material_source']
    record['ao_height_source'] = record['normal_source'] if edition == 'java' else None
    record['pbr_conversion'] = {
        key: str(value.relative_to(SOURCE)) if isinstance(value, Path) else value
        for key, value in conversion.items()
    }
    for field in ['normal_source', 'material_source']:
        if field in conversion:
            record[field] = str(conversion[field].relative_to(SOURCE))
    if 'material_source' in conversion:
        record['smoothness_source'] = record['material_source']
    elif 'material_scalar' in conversion:
        record['smoothness_source'] = None
    record['albedo_processing'] = 'sRGB color; linear-light alpha-weighted resizing and compositing'
    java_candidate = JAVA / area / (stem + '.png')
    if edition == 'bedrock' and java_candidate.exists():
        PROVENANCE[-1]['selection_note'] = f'Java {stem}.png is wholly transparent; selected visible Bedrock {path.suffix[1:].upper()}.'
    return key


def swatch(texture):
    a = np.array(Image.open(ROOT / TEXTURES[texture]['path']).convert('RGBA')).astype(float)
    mask = a[:,:,3] >= 128
    rgb = a[:,:,:3][mask].mean(axis=0) / 255 if mask.any() else np.array([.5,.5,.5])
    return [round(float(v),6) for v in rgb] + [1.]


def block(key, side=None, top=None, bottom=None, *, kind='cube', family='stone', upper=None, tint=None, supports=False, emission=0):
    cutout = kind in ['leaves','plant','tall_plant']
    folder = 'foliage' if cutout else 'blocks'
    side = import_texture(side or key, folder=folder, cutout=cutout, tint=tint)
    top = import_texture(top, folder=folder, cutout=cutout, tint=tint) if top else side
    bottom = import_texture(bottom, folder=folder, cutout=cutout, tint=tint) if bottom else top
    if key not in BLOCK_IDS:
        BLOCK_IDS[key] = max(BLOCK_IDS.values(), default=-1) + 1
    d = {'id':BLOCK_IDS[key],'key':key,'name':key.replace('_',' ').upper(),'top':top,'side':side,'bottom':bottom,
         'kind':kind,'supports_plant':supports,'flammable':family in ['wood','leaves','plant'],
         'emission':emission,'swatch':swatch(side),'reflectance':[int(v*255) for v in swatch(side)[:3]],'family':family}
    if upper:
        d['top_half'] = import_texture(upper, folder='foliage', cutout=True, tint=tint)
    if emission:
        for t in [side,top,bottom]: TEXTURES[t]['emission_strength'] = 3.5 * emission / 15
    BLOCKS.append(d)


def selected_blocks():
    for name in ['coarse_dirt','rooted_dirt','mud','packed_mud','clay','red_sand','pale_moss_block','powder_snow','ice','packed_ice','blue_ice','bedrock']:
        block(name, family='terrain', supports=name not in ['ice','packed_ice','blue_ice','bedrock'])
    block('dirt_path','dirt_path_side','dirt_path_top','dirt',family='terrain',supports=True)
    for name in ['podzol','mycelium']:
        block(name,name+'_side',name+'_top','dirt',family='terrain',supports=True)
    for species in ['oak','spruce','birch','jungle','acacia','dark_oak','mangrove','cherry','pale_oak']:
        for prefix in ['', 'stripped_']:
            key=prefix+species+'_log'
            block(key,key,key+'_top',kind='log',family='wood')
        block(species+'_planks',family='wood')
        tint = {'oak':(115,168,68),'spruce':(88,127,91),'birch':(139,169,80),
                'jungle':(91,158,64),'acacia':(115,166,72),'dark_oak':(87,139,55),
                'mangrove':(110,149,56)}.get(species)
        block(species+'_leaves',kind='leaves',family='leaves',tint=tint)
    for name in ['azalea_leaves','flowering_azalea_leaves']:
        block(name,kind='leaves',family='leaves')
    block('mangrove_roots','mangrove_roots_side','mangrove_roots_top',family='wood')
    block('muddy_mangrove_roots','muddy_mangrove_roots_side','muddy_mangrove_roots_top',family='wood')
    for name in ['bamboo_block','stripped_bamboo_block']:
        block(name,top=name+'_top',kind='log',family='wood')
    for name in ['bamboo_planks','bamboo_mosaic']: block(name,family='wood')
    for name in ['smooth_stone','cobblestone','mossy_cobblestone','granite','polished_granite','diorite','polished_diorite',
                 'andesite','polished_andesite','cobbled_deepslate','polished_deepslate','chiseled_deepslate',
                 'tuff','polished_tuff','calcite','dripstone_block','smooth_basalt']:
        block(name)
    block('deepslate',top='deepslate_top')
    block('reinforced_deepslate','reinforced_deepslate_side','reinforced_deepslate_top','reinforced_deepslate_bottom')
    block('chiseled_tuff',top='chiseled_tuff_top')
    for name in ['basalt','polished_basalt']: block(name,name+'_side',name+'_top',kind='log',family='stone')
    for color in ['', 'red_']:
        name=color+'sandstone'
        block(name,top=name+'_top',bottom=name+'_bottom')
        for variant in ['cut_','chiseled_']:
            block(variant+name,top=name+'_top',bottom=name+'_bottom')
        block('smooth_'+name,name+'_top')
    for name in ['bricks','stone_bricks','mossy_stone_bricks','cracked_stone_bricks','chiseled_stone_bricks',
                 'deepslate_bricks','cracked_deepslate_bricks','deepslate_tiles','cracked_deepslate_tiles',
                 'tuff_bricks','mud_bricks','resin_bricks','chiseled_resin_bricks']:
        block(name,family='masonry')
    block('chiseled_tuff_bricks',top='chiseled_tuff_bricks_top',family='masonry')
    block('terracotta',family='terracotta')
    for color in ['white','orange','magenta','light_blue','yellow','lime','pink','gray','light_gray','cyan','purple','blue','brown','green','red','black']:
        for variant in ['terracotta','glazed_terracotta']: block(color+'_'+variant,family='terracotta')
    for name in ['dandelion','poppy','blue_orchid','allium','azure_bluet','oxeye_daisy','cornflower','lily_of_the_valley',
                 'wither_rose','red_tulip','orange_tulip','white_tulip','pink_tulip','torchflower']:
        block(name,kind='plant',family='plant')
    for name in ['sunflower','lilac','rose_bush','peony']:
        block(name,name+'_bottom',kind='tall_plant',upper=name+'_top',family='plant')
    block('pitcher_plant','pitcher_crop_bottom_stage_4',kind='tall_plant',upper='pitcher_crop_top_stage_4',family='plant')
    for name in ['pink_petals','spore_blossom','azalea_plant','short_grass','dry_grass','tall_dry_grass','bush','dead_bush',
                 'firefly_bush','vine','cave_vines','hanging_roots','glow_lichen','lily_pad','seagrass',
                 'brown_mushroom','red_mushroom','sugar_cane','leaf_litter','pale_hanging_moss_tip']:
        try: block(name,kind='plant',family='plant',tint=(111,166,67) if name in ['short_grass','vine','lily_pad','sugar_cane'] else None, emission=5 if name in ['firefly_bush','glow_lichen'] else 0)
        except FileNotFoundError:
            if name == 'dry_grass': print('Unavailable: short dry grass; tall dry grass remains imported')
            else: raise
    block('flowering_azalea','flowering_azalea_side',top='flowering_azalea_top',kind='leaves',family='plant')
    block('azalea','azalea_side',top='azalea_top',kind='leaves',family='plant')
    block('sweet_berry_bush','sweet_berry_bush_stage3',kind='plant',family='plant')
    block('cave_vines_berries','cave_vines_head_berries',kind='plant',family='plant',emission=10)
    for name,texture in [('large_fern','large_fern'),('tall_grass_plant','tall_grass'),('tall_seagrass','tall_seagrass')]:
        block(name,texture+'_bottom',kind='tall_plant',upper=texture+'_top',family='plant',tint=(111,166,67) if name != 'tall_seagrass' else None)
    for species in ['oak','spruce','birch','jungle','acacia','dark_oak','cherry']:
        block(species+'_sapling',kind='plant',family='plant')
    block('mangrove_propagule',kind='plant',family='plant')
    for mineral in ['coal','iron','copper','gold','lapis','redstone','diamond','emerald']:
        for prefix in ['', 'deepslate_']: block(prefix+mineral+'_ore',family='ore')
    for name in ['amethyst_block','budding_amethyst']: block(name,family='crystal')
    for name in ['small_amethyst_bud','medium_amethyst_bud','large_amethyst_bud','amethyst_cluster']:
        block(name,kind='plant',family='crystal',emission=5)
    for name in ['polished_blackstone','chiseled_polished_blackstone','polished_blackstone_bricks',
                 'cracked_polished_blackstone_bricks','gilded_blackstone','obsidian','crying_obsidian','magma',
                 'netherrack','soul_sand','soul_soil','prismarine','prismarine_bricks','dark_prismarine','end_stone',
                 'end_stone_bricks','purpur_block','nether_bricks','red_nether_bricks','cracked_nether_bricks',
                 'chiseled_nether_bricks','quartz_bricks','cinnabar','polished_cinnabar','chiseled_cinnabar',
                 'cinnabar_bricks','sulfur_bricks','chiseled_sulfur']:
        block(name,family='fantasy',emission=10 if name == 'crying_obsidian' else 3 if name == 'magma' else 0)
    block('blackstone',top='blackstone_top',family='fantasy')
    block('quartz_block','quartz_block_side','quartz_block_top','quartz_block_bottom',family='fantasy')
    block('smooth_quartz','quartz_block_bottom',family='fantasy')
    block('chiseled_quartz_block',top='chiseled_quartz_block_top',family='fantasy')
    block('quartz_pillar',top='quartz_pillar_top',kind='log',family='fantasy')
    block('purpur_pillar',top='purpur_pillar_top',kind='log',family='fantasy')
    for wood in ['crimson','warped']:
        for prefix in ['', 'stripped_']:
            name=prefix+wood+'_stem'
            block(name,top=name+'_top',kind='log',family='wood')
        block(wood+'_planks',family='wood')
        block(wood+'_nylium',wood+'_nylium_side',wood+'_nylium','netherrack',family='fantasy',supports=True)
        for suffix in ['fungus','roots']: block(wood+'_'+suffix,kind='plant',family='plant')
    for name in ['weeping_vines','twisting_vines','nether_sprouts','chorus_flower']:
        block(name,kind='plant',family='plant')
    for name in ['nether_gold_ore','nether_quartz_ore']: block(name,family='ore')
    block('ancient_debris','ancient_debris_side','ancient_debris_top',family='ore')
    for name in ['shroomlight','sea_lantern']: block(name,family='fantasy',emission=15)


def replacements():
    pairs = {'grass_top':'grass_block_top','grass_side':'grass_block_side','dirt':'dirt','stone':'stone',
             'sand':'sand','snow':'snow','moss':'moss_block','gravel':'gravel','glowstone':'glowstone',
             'wood_side':'oak_log','wood_top':'oak_log_top','leaves':'oak_leaves','flower_red':'poppy',
             'flower_yellow':'dandelion','flower_blue':'blue_orchid','fern':'fern','tall_grass':'short_grass',
             'sapling':'oak_sapling','kiln_brick':'bricks',
             'kiln_vent':'furnace_front','kiln_lit':'furnace_front_on','hopper_side':'hopper_outside',
             'hopper_top':'hopper_top','chest_side':'barrel_side','chest_top':'barrel_top','water':'water_still'}
    for dest,src in pairs.items():
        folder='foliage' if dest in ['leaves','flower_red','flower_yellow','flower_blue','fern','tall_grass'] else 'items' if dest in ['seeds','sapling','stick'] else 'blocks'
        tint=(115,168,68) if dest in ['grass_top','leaves','tall_grass','fern'] else None
        key=import_texture(src,key=dest,folder=folder,cutout=folder != 'blocks',tint=tint,area='item' if dest in ['seeds','stick'] else 'block')
        if dest == 'grass_side':
            # Compose tinted overlay against the source dirt, preserving opaque block faces.
            overlay=frame(JAVA/'block/grass_block_side_overlay.png')
            a=np.array(overlay); a[:,:,:3]=np.rint(a[:,:,:3].astype(float)*np.array([115,168,68])/255).astype(np.uint8)
            color=Image.open(ROOT/TEXTURES[key]['path']).convert('RGBA')
            color = albedo.composite(color, Image.fromarray(a))
            color.putalpha(255); save_png(color, ROOT/TEXTURES[key]['path'])


def compose_sunflower():
    # Minecraft uses extra oriented flower geometry. Bake the head into the upper
    # cross-plane sprite so this renderer shows a complete two-block sunflower.
    front = import_texture('sunflower_front', folder='foliage', cutout=True)
    top_path = ROOT / TEXTURES['jg_sunflower_top']['path']
    front_path = ROOT / TEXTURES[front]['path']
    width = Image.open(top_path).width
    head_size = round(width * .6)
    head = albedo.resize(Image.open(front_path), (head_size, head_size))
    mask = head.getchannel('A')
    for suffix in ['', '_n', '_s']:
        dest = top_path.with_stem(top_path.stem + suffix)
        src = front_path.with_stem(front_path.stem + suffix)
        base = Image.open(dest).convert('RGBA')
        piece = (pbr.data_image(src, (head_size, head_size)) if suffix else
                 albedo.resize(Image.open(src), (head_size, head_size)))
        if suffix:
            base.paste(piece, ((width-head_size)//2, 0), mask)
        else:
            base = albedo.composite(base, piece, ((width-head_size)//2, 0))
        save_png(base, dest)
    next(p for p in PROVENANCE if p['destination'] == str(top_path.relative_to(ROOT)))['composite'] = 'sunflower_front head at 60% tile width, centered at top'


def main():
    global SOURCE,JAVA,BEDROCK
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source',type=Path,nargs='?')
    parser.add_argument('--icons-only',action='store_true',help='Regenerate source-art inventory icons from existing imported assets')
    args=parser.parse_args()
    if args.icons_only:
        subprocess.run([sys.executable,str(Path(__file__).with_name('icons.py'))],check=True)
        return
    if args.source is None:
        parser.error('source checkout is required unless --icons-only is used')
    SOURCE=args.source.resolve()
    ids_path = ROOT/'tools/jg_rtx/block_ids.json'
    if ids_path.exists():
        BLOCK_IDS.update(json.loads(ids_path.read_text()))
    JAVA=SOURCE/'java/pack/assets/minecraft/textures'
    BEDROCK=SOURCE/'bedrock/pack/RP/textures/blocks'
    replacements(); selected_blocks(); compose_sunflower()
    textures=[]
    for t in TEXTURES.values():
        companions=t.pop('_companions')
        if t['key'].startswith('jg_'): textures.append(t)
        # Original companion identities remain handled by companions.rs; append new builtin maps.
        original_companion=t['key'] in ['grass_top','grass_side','dirt','stone','sand','snow','moss','gravel','glowstone','wood_side','wood_top','stick']
        if not original_companion: textures.extend(companions)
        elif t['key'].startswith('jg_'): textures.extend(companions)
    out=ROOT/'assets/jg-rtx'
    out.mkdir(exist_ok=True)
    revision=subprocess.run(['git','-C',str(SOURCE),'rev-parse','HEAD'],capture_output=True,text=True).stdout.strip()
    manifest={'source':'https://github.com/jasonjgardner/jg-rtx','source_revision':revision,'textures':textures,'blocks':BLOCKS}
    ids_path.write_text(json.dumps(BLOCK_IDS,indent=2)+'\n')
    (out/'catalog.json').write_text(json.dumps(manifest,indent=2)+'\n')
    (out/'provenance.json').write_text(json.dumps(PROVENANCE,indent=2)+'\n')
    shutil.copyfile(SOURCE/'LICENSE',out/'LICENSE')
    shutil.copyfile(SOURCE/'docs/CREDITS.md',out/'CREDITS.md')
    lines=['// Generated by tools/jg_rtx/import.py. Do not edit.','pub(super) const TEXTURES: &[(&str, &[u8])] = &[']
    for t in textures:
        lines.append('    ("'+t['key']+'", include_bytes!("../../../'+t['path']+'")),')
    lines.append('];')
    generated = ROOT/'src/content/jg_rtx/assets.rs'
    generated.write_text('\n'.join(lines)+'\n')
    subprocess.run(['rustfmt', str(generated)], check=True)
    subprocess.run([sys.executable,str(Path(__file__).with_name('icons.py'))],check=True)
    print(f'Imported {len(BLOCKS)} block types; {len(textures)} appended texture layers; {len(PROVENANCE)} albedo sources.')

if __name__=='__main__': main()
