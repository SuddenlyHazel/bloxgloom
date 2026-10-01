"""Structural checks on actual source assets and their checked-in conversion."""
import json
import struct
import unittest

from convert import DEST, Glb, transform


class AssetTests(unittest.TestCase):
    def setUp(self):
        self.asset = json.loads((DEST / "character.json").read_text())

    def test_counts_and_rigid_weights(self):
        self.assertEqual(len(self.asset["joints"]), 7)
        self.assertEqual(len(self.asset["vertices"]), 576)
        self.assertEqual(len(self.asset["indices"]), 576)
        body = Glb(DEST / "source/player.glb")
        for mesh in body.doc["meshes"]:
            for primitive in mesh["primitives"]:
                weights = body.accessor(primitive["attributes"]["WEIGHTS_0"])
                self.assertTrue(all(weight == [1, 0, 0, 0] for weight in weights))

    def test_inverse_bind_cancels_rest_translation(self):
        body = Glb(DEST / "source/player.glb")
        skin = body.doc["skins"][0]
        inverse = body.accessor(skin["inverseBindMatrices"])
        for joint, matrix in zip(self.asset["joints"], inverse):
            self.assertEqual(transform(matrix, joint["translation"], 1), [0, 0, 0])
        vertices = [v for v in self.asset["vertices"] if v["material"] == 0]
        heights = [v["position"][1] + self.asset["joints"][v["joint"]]["translation"][1] for v in vertices]
        self.assertAlmostEqual(min(heights), 0)
        self.assertAlmostEqual(max(heights) * 0.9, 1.8)

    def test_every_body_vertex_reconstructs_source_bind_and_uv(self):
        source = Glb(DEST / "source/player.glb")
        converted = iter(v for v in self.asset["vertices"] if v["material"] == 0)
        for mesh in source.doc["meshes"]:
            for primitive in mesh["primitives"]:
                attrs = primitive["attributes"]
                for position, normal, uv in zip(*(source.accessor(attrs[name]) for name in ["POSITION", "NORMAL", "TEXCOORD_0"])):
                    vertex = next(converted)
                    rest = self.asset["joints"][vertex["joint"]]["translation"]
                    for actual, expected in zip([a + b for a, b in zip(vertex["position"], rest)], position):
                        self.assertAlmostEqual(actual, expected, places=6)
                    self.assertEqual(vertex["normal"], normal)
                    self.assertEqual(vertex["uv"], uv)
        self.assertIsNone(next(converted, None))

    def test_tool_clip_anatomical_mapping_preserves_legacy_names(self):
        clips = {clip["name"]: clip for clip in self.asset["clips"]}
        for clip_name, moving, stationary in [("tool_use_left", "right_arm", "left_arm"), ("tool_use_right", "left_arm", "right_arm")]:
            movement = {}
            for channel in clips[clip_name]["channels"]:
                if channel["path"] == "rotation":
                    movement[self.asset["joints"][channel["joint"]]["name"]] = max(sum(v * v for v in value[:3]) for value in channel["values"])
            self.assertGreater(movement[moving], 0.5)
            self.assertEqual(movement[stationary], 0)

    def test_textures_are_byte_exact_and_keep_aspect(self):
        for name, source, size in [("body", "player", (512, 256)), ("hair", "hair", (32, 32))]:
            png = (DEST / f"{name}.png").read_bytes()
            self.assertEqual(png, Glb(DEST / f"source/{source}.glb").png())
            self.assertEqual(struct.unpack_from(">II", png, 16), size)

    def test_clip_semantics_and_hair_attachment(self):
        clips = {clip["name"]: clip for clip in self.asset["clips"]}
        self.assertEqual(set(clips), {"walk", "idle", "crouch", "tool_use_left", "tool_use_right"})
        self.assertTrue(clips["walk"]["looping"])
        self.assertTrue(clips["idle"]["looping"])
        self.assertFalse(clips["crouch"]["looping"])
        self.assertAlmostEqual(clips["crouch"]["duration"], 0.6)
        hair = [v for v in self.asset["vertices"] if v["material"] == 1]
        self.assertTrue(hair)
        self.assertTrue(all(v["joint"] == 1 for v in hair))
        self.assertEqual(self.asset["joints"][1]["name"], "head")


if __name__ == "__main__":
    unittest.main()
