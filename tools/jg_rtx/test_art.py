import unittest
import numpy as np
from PIL import Image
import art


class DerivedArtTests(unittest.TestCase):
    def test_diffuse_curation_preserves_alpha_hue_and_unselected_art(self):
        pixels = np.full((8, 8, 4), [80, 60, 40, 255], dtype=np.uint8)
        pixels[3, 4] = [200, 150, 100, 37]
        source = Image.fromarray(pixels)
        changed, provenance = art.bark(source, 'jg_cherry_log')
        output = np.array(changed)
        self.assertTrue(np.array_equal(output[:, :, 3], pixels[:, :, 3]))
        self.assertTrue(np.array_equal(output[0, 0], pixels[0, 0]))
        self.assertLess(output[3, 4, 0], pixels[3, 4, 0])
        self.assertGreater(output[3, 4, 0], output[3, 4, 1])
        self.assertEqual(provenance['kind'], 'derived diffuse art')
        unchanged, provenance = art.bark(source, 'jg_bricks')
        self.assertIs(unchanged, source)
        self.assertIsNone(provenance)

    def test_gloss_keeps_categorical_channels_and_nonbotanical_materials_exact(self):
        pixels = np.full((8, 8, 4), [200, 10, 191, 255], dtype=np.uint8)
        source = Image.fromarray(pixels)
        changed, provenance = art.gloss(source, 'cherry_leaves', True)
        output = np.array(changed)
        self.assertTrue(np.array_equal(output[:, :, 1:], pixels[:, :, 1:]))
        self.assertEqual(output[:, :, 0].max(), 128)
        self.assertIsNotNone(provenance)
        for stem, cutout in [('amethyst_cluster', True), ('cherry_leaves', False)]:
            unchanged, note = art.gloss(source, stem, cutout)
            self.assertIs(unchanged, source)
            self.assertIsNone(note)


if __name__ == '__main__':
    unittest.main()
