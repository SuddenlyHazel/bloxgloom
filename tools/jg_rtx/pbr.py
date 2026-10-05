"""Canonical JG RTX companion sources and LabPBR data-channel conversion.

Edition aliases and material rules follow JG RTX's src/scripts/labpbr modules.
Albedo is handled separately from linear normal and material data.
"""
import json
import re
from functools import lru_cache
from pathlib import Path

import numpy as np
from PIL import Image

EDITION_NAMES = json.loads(Path(__file__).with_name('edition_names.json').read_text())
MER = 'metalness_emissive_roughness'
MERS = MER + '_subsurface'


def data_image(path, size):
    """Resize independent linear data; alpha is not a transparency weight."""
    image = Image.open(path).convert('RGBA')
    if image.height > image.width and image.height % image.width == 0:
        image = image.crop((0, 0, image.width, image.width))
    return Image.merge('RGBA', tuple(
        image.getchannel(channel).resize(size, Image.Resampling.LANCZOS)
        for channel in 'RGBA'
    ))


def texture_path(directory, stem):
    if not isinstance(stem, str):
        return None
    for extension in ('.png', '.tga'):
        path = directory / (stem + extension)
        if path.exists():
            return path
    return None


@lru_cache(maxsize=None)
def texture_sets(directory):
    index = {}
    for path in sorted(directory.rglob('*.texture_set.json')):
        name = path.name.removesuffix('.texture_set.json')
        if name in index:
            raise ValueError(f'Ambiguous texture set {name}: {index[name]} and {path}')
        index[name] = path
    return index


def canonical_sources(directory, stem):
    """Honor texture-set semantics, including scalar MER and mislabeled files."""
    name = EDITION_NAMES.get(stem, stem)
    path = texture_sets(directory).get(name)
    if path:
        definition = json.loads(path.read_text(encoding='utf-8-sig'))['minecraft:texture_set']
        directory = path.parent
    else:
        definition = {}
    has_subsurface = MERS in definition
    mer = definition.get(MERS if has_subsurface else MER)
    if isinstance(mer, list):
        material = mer
    else:
        material = texture_path(directory, mer)
        if material is None and isinstance(mer, str):
            alternate = mer[:-1] if mer.endswith('_mers') else mer + 's'
            material = texture_path(directory, alternate)
        if not definition:
            material = texture_path(directory, name + '_mer')
            if material is None:
                material = texture_path(directory, name + '_mers')
                has_subsurface = material is not None
    normal = texture_path(directory, definition.get('normal', name + '_normal'))
    height = texture_path(directory, definition.get('heightmap'))
    return name, normal, material, height, has_subsurface, path


def metal_rule(name):
    # The upstream pack authors some genuine copper below 0.5 metalness.
    if 'copper' in name or name == 'lightning_rod':
        return 234, .2
    if 'gold' in name or 'gilded' in name:
        return 231, .5
    if name.startswith('anvil'):
        return 230, .125
    if name.startswith('lodestone'):
        return 230, .25
    if name in ('grindstone_pivot', 'trip_wire'):
        return 230, .125
    if re.search(r'iron|^cauldron|^hopper|^rail_|^chain\d*$|^bell_', name):
        return 230, .5
    if name.startswith('honey_') or name == 'tinted_glass':
        return 10, float('inf')
    return 255, .75 if name.startswith('vault_') else .5


def porosity(name):
    rules = (
        (r'^glazed_terracotta|glass|^ice$|^ice_packed$|^blue_ice$|^frosted_ice|^obsidian|^crying_obsidian$|^bedrock$|^quartz_block|^amethyst|^budding_amethyst$|^calcite$|copper|gold|gilded|iron|^anvil|^cauldron|^hopper|^rail_|^chain\d*$|^lodestone|^netherite_block$|^ancient_debris|^diamond_block$|^emerald_block$|^lapis_block$|^redstone_block$|^slime$|^honey_|^sea_lantern$|^glowstone$|^beacon$|^conduit', 0),
        (r'^sponge', 64),
        (r'^wool_colored|_wool$|^carpet|^snow$|^powder_snow$|^gravel$', 45),
        (r'^concrete_powder', 50),
        (r'^sand$|^red_sand$|^dirt|^coarse_dirt$|^clay$|^mud|^soul_sand$|^soul_soil$|^farmland|^grass_path|^mycelium|^podzol|^hay_block|^dried_kelp|^bone_block', 40),
        (r'^netherrack$|nether_wart_block$|^shroomlight$|nylium', 30),
        (r'^planks_|^log_|^stripped_|_log$|_planks$|^door_|trapdoor$|^bookshelf$|^crafting_table|^barrel', 20),
        (r'^hardened_clay|terracotta', 16),
        (r'^stone|^cobblestone|^deepslate|^granite|^diorite|^andesite|^tuff|^basalt|^blackstone|^end_stone|sandstone|^prismarine|^purpur|^brick$|brick_|_brick$|^nether_brick', 12),
    )
    return next((value for pattern, value in rules if re.search(pattern, name)), 0)


def encode_material(mer, name, original, has_subsurface):
    """Convert perceptual MER roughness and encode the LabPBR channel contract."""
    result = np.array(original).copy()
    result[:, :, 0] = 255 - mer[:, :, 2]
    target, threshold = metal_rule(name)
    result[:, :, 1] = np.where(mer[:, :, 0] >= threshold * 255, target, 10)
    result[:, :, 2] = (65 + np.rint(mer[:, :, 3].astype(float) * 190 / 255)
                       if has_subsurface else porosity(name))
    result[:, :, 3] = (np.rint(mer[:, :, 1].astype(float) * 254 / 255)
                       if np.any(mer[:, :, 1]) else 255)
    return Image.fromarray(result.astype(np.uint8))


def repair_companions(directory, stem, normal, specular, java_normal, size):
    name, n_path, mer, height, sss, texture_set = canonical_sources(directory, stem)
    details = {'normal_convention': 'DirectX image-down'}
    if texture_set:
        details['texture_set'] = texture_set
    # Keep AO/height from Java but replace XY with the canonical Bedrock direction.
    data = np.array(data_image(java_normal, size) if java_normal else normal).copy()
    if n_path:
        canonical = np.array(data_image(n_path, size))
        data[:, :, :2] = canonical[:, :, :2]
        details['normal_source'] = n_path
    else:
        details['normal_fallback'] = 'Java LabPBR DirectX' if java_normal else 'flat normal'
    if java_normal is None and height:
        data[:, :, 3] = np.array(data_image(height, size))[:, :, 0]
        details['height_source'] = height
    normal = Image.fromarray(data)
    if mer is not None:
        if isinstance(mer, list):
            pixels = np.empty((size[1], size[0], 4), dtype=np.uint8)
            pixels[:] = [*mer[:3], 255]
            details['material_scalar'] = mer[:3]
        else:
            pixels = np.array(data_image(mer, size))
            details['material_source'] = mer
        specular = encode_material(pixels, name, specular, sss)
        details['smoothness'] = '255 minus canonical MER perceptual roughness'
        details['material_encoding'] = 'MER to LabPBR 1.3 with curated F0 and metal rules'
    else:
        data = np.array(specular).copy()
        data[:, :, 1] = np.where(data[:, :, 1] == 0, 10, data[:, :, 1])
        specular = Image.fromarray(data)
        details['material_fallback'] = 'Java LabPBR with zero F0 corrected to dielectric 4 percent'
        details['smoothness'] = 'authored Java LabPBR fallback; no canonical MER available'
    return normal, specular, details
