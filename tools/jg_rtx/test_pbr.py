"""Regression checks for source semantics and independent PBR data channels."""
import json
import tempfile
import unittest
from pathlib import Path

import numpy as np
from PIL import Image

import pbr


class ConversionTests(unittest.TestCase):
    def test_ore_presets_and_low_copper_metalness(self):
        original = Image.new('RGBA', (2, 1), (177, 0, 19, 255))
        for name, target, byte in [('iron_ore', 230, 195), ('gold_ore', 231, 195),
                                   ('copper_ore', 234, 77)]:
            mer = np.array([[[0, 0, 120, 255], [byte, 0, 120, 255]]], dtype=np.uint8)
            result = np.array(pbr.encode_material(mer, name, original, False))
            self.assertEqual(result[0, :, 1].tolist(), [10, target])
            self.assertEqual(result[0, :, 0].tolist(), [177, 177], 'stage 1 preserves smoothness')
            self.assertTrue(np.all(result[:, :, 3] == 255))

    def test_subsurface_and_emission_have_distinct_sentinels(self):
        mer = np.array([[[0, 0, 0, 0], [0, 255, 0, 255]]], dtype=np.uint8)
        original = Image.new('RGBA', (2, 1), (99, 0, 19, 255))
        result = np.array(pbr.encode_material(mer, 'leaves_oak', original, True))
        self.assertEqual(result[0, :, 2].tolist(), [65, 255])
        self.assertEqual(result[0, :, 3].tolist(), [0, 254])
        result = np.array(pbr.encode_material(mer, 'stone', original, False))
        self.assertEqual(result[0, :, 2].tolist(), [12, 12])

    def test_data_resize_does_not_premultiply_by_height(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'normal.png'
            Image.new('RGBA', (4, 4), (80, 200, 240, 0)).save(path)
            result = np.array(pbr.data_image(path, (2, 2)))
            self.assertTrue(np.all(result == [80, 200, 240, 0]))

    def test_texture_sets_resolve_nested_names_scalar_mer_and_mislabeled_tga(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            nested = root / 'deepslate'
            nested.mkdir()
            path = nested / 'deepslate_iron_ore.texture_set.json'
            path.write_text(json.dumps({'minecraft:texture_set': {
                pbr.MERS: 'ore_mers', 'normal': 'ore_normal'}}))
            Image.new('RGBA', (2, 2), (195, 0, 70, 127)).save(nested / 'ore_mer.tga')
            Image.new('RGBA', (2, 2), (80, 200, 255, 255)).save(nested / 'ore_normal.png')
            name, normal, mer, height, sss, texture_set = pbr.canonical_sources(root, 'deepslate_iron_ore')
            self.assertEqual(name, 'deepslate_iron_ore')
            self.assertEqual(normal, nested / 'ore_normal.png')
            self.assertEqual(mer, nested / 'ore_mer.tga')
            self.assertIsNone(height)
            self.assertTrue(sss)
            self.assertEqual(texture_set, path)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'stone.texture_set.json').write_text(json.dumps({
                'minecraft:texture_set': {pbr.MER: [0, 0, 210]}}))
            self.assertEqual(pbr.canonical_sources(root, 'stone')[2], [0, 0, 210])

    def test_canonical_normal_keeps_directx_slopes_and_java_ao_height(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            Image.new('RGBA', (2, 2), (80, 200, 255, 255)).save(root / 'stone_normal.png')
            java = root / 'stone_n.png'
            Image.new('RGBA', (2, 2), (80, 55, 77, 210)).save(java)
            with Image.open(java) as java_image:
                normal, material, details = pbr.repair_companions(
                    root, 'stone', java_image, Image.new('RGBA', (2, 2), (177, 0, 19, 255)),
                    java, (2, 2))
            self.assertEqual(normal.getpixel((0, 0)), (80, 200, 77, 210))
            self.assertEqual(material.getpixel((0, 0)), (177, 10, 19, 255))
            self.assertEqual(details['normal_source'], root / 'stone_normal.png')


if __name__ == '__main__':
    unittest.main()
