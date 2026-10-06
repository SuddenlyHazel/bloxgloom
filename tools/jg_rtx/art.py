"""Small, reproducible derived-art edits; source checkout is never modified.

Curated bark reduces isolated photographic glints in diffuse color, leaving
grooves, hues and authored companions intact. Botanical materials use a dry
leaf/petal gloss ceiling rather than the pack's wet cherry-leaf highlight.
"""
import numpy as np
from PIL import Image
from color import srgb_to_linear, rgba_from_linear

BARK = {'wood_side', 'jg_oak_log', 'jg_cherry_log'}


def bark(image, key):
    if key not in BARK:
        return image, None
    data = np.array(image.convert('RGBA')).astype(float) / 255
    rgb = srgb_to_linear(data[:, :, :3])
    luminance = rgb @ np.array([.2126, .7152, .0722])
    neighbors = np.stack([np.roll(np.roll(luminance, y, 0), x, 1)
                          for y in [-1, 0, 1] for x in [-1, 0, 1]])
    median = np.median(neighbors, axis=0)
    # Soften only tiny positive outliers: broad lichen patches and shadowed
    # grooves remain. Periodic neighborhoods keep tile seams continuous.
    target = luminance - .45 * np.maximum(luminance-median, 0.)
    ratio = np.divide(target, luminance, out=np.ones_like(target), where=luminance > 1e-8)
    result = rgba_from_linear(rgb * ratio[:, :, None], data[:, :, 3])
    return result, {'kind': 'derived diffuse art', 'operation':
                    'periodic 3x3 linear-luminance median; suppress 45% of isolated positive glints; preserve chromaticity and alpha',
                    'tool': 'tools/jg_rtx/art.py'}


def botanical(stem):
    return (('leaves' in stem) or stem in {
        'fern', 'large_fern_bottom', 'large_fern_top', 'short_grass', 'tall_grass_bottom',
        'tall_grass_top', 'dry_grass', 'short_dry_grass', 'dead_bush', 'deadbush',
        'poppy', 'dandelion', 'blue_orchid', 'allium', 'azure_bluet', 'oxeye_daisy',
        'cornflower', 'lily_of_the_valley', 'wither_rose', 'pink_petals', 'wildflowers',
        'bush', 'firefly_bush', 'nether_sprouts', 'weeping_vines', 'twisting_vines',
        'oak_sapling', 'crimson_roots', 'warped_roots', 'crimson_fungus', 'warped_fungus',
        'vine', 'cave_vines', 'cave_vines_head_berries', 'hanging_roots', 'lily_pad',
        'seagrass', 'tall_seagrass_bottom', 'tall_seagrass_top', 'brown_mushroom',
        'red_mushroom', 'sugar_cane', 'leaf_litter', 'pale_hanging_moss_tip',
        'mangrove_propagule', 'sweet_berry_bush_stage3', 'spore_blossom', 'torchflower',
        'chorus_flower', 'azalea_plant', 'tall_dry_grass'}
        or any(part in stem for part in ['tulip', 'sunflower', 'rose_bush', 'peony', 'lilac',
                                       '_sapling', 'azalea_', 'pitcher_crop']))


def gloss(image, stem, cutout):
    if not cutout or not botanical(stem):
        return image, None
    data = np.array(image.convert('RGBA'))
    maximum = 128 if 'leaves' in stem else 153
    if data[:, :, 0].max() <= maximum:
        return image, None
    data[:, :, 0] = np.minimum(data[:, :, 0], maximum)
    return Image.fromarray(data), {'kind': 'derived material art', 'operation':
                                  f'dry botanical perceptual smoothness ceiling {maximum}/255; retain source F0, subsurface and emission',
                                  'tool': 'tools/jg_rtx/art.py'}
