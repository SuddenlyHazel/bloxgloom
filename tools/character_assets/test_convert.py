"""Structural checks on actual source assets and their checked-in conversion."""
import copy
import hashlib
import json
from pathlib import Path
import shutil
import tempfile
import struct
import unittest

from convert import DEST, HAIR_CATALOG, Glb, convert, load_native, geometry_animation_sha256, transform, validate_geometry


class AssetTests(unittest.TestCase):
    def setUp(self):
        self.asset = load_native()

    def test_counts_and_rigid_weights(self):
        self.assertEqual(len(self.asset["joints"]), 7)
        self.assertEqual(len(self.asset["vertices"]), 22680)
        self.assertEqual(len(self.asset["indices"]), 22680)
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
        for name, source, size in [("body", "player", (512, 256))] + [(entry[2], entry[2], (32, 32)) for entry in HAIR_CATALOG]:
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
        hair = [v for v in self.asset["vertices"] if v["material"] > 0]
        self.assertTrue(hair)
        self.assertTrue(all(v["joint"] == 1 for v in hair))
        self.assertEqual(self.asset["joints"][1]["name"], "head")


    def test_all_hair_vertices_preserve_source_head_local_normals_and_uvs(self):
        for (material, _, name, _, lo, hi), count in zip(HAIR_CATALOG, [360, 504, 792, 2664, 1764, 360, 432, 2772, 1980, 2412, 4248, 3168, 1008]):
            source = Glb(DEST / f"source/{name}.glb")
            converted = [v for v in self.asset["vertices"] if v["material"] == material]
            self.assertEqual(len(converted), count)
            expected = []
            for mesh in source.doc["meshes"]:
                for primitive in mesh["primitives"]:
                    attrs = primitive["attributes"]
                    expected.extend(zip(*(source.accessor(attrs[key]) for key in ["POSITION", "NORMAL", "TEXCOORD_0"])))
            for vertex, (position, normal, uv) in zip(converted, expected):
                self.assertEqual(vertex["position"], position)
                self.assertEqual(vertex["normal"], normal)
                self.assertEqual(vertex["uv"], uv)
                self.assertEqual(vertex["joint"], 1)
                self.assertTrue(all(lo[axis] <= position[axis] <= hi[axis] for axis in range(3)))
            self.assertEqual(len(expected), len(converted))

    def test_conversion_is_deterministic_and_texture_exact(self):
        with tempfile.TemporaryDirectory() as directory:
            dest = Path(directory)
            shutil.copytree(DEST / "source", dest / "source")
            convert(dest)
            for name in ["character.json", "body.png", "hair_sockets.json"] + [f"clip_{name}.json" for name in ["walk", "idle", "crouch", "tool_use_left", "tool_use_right"]] + [entry[2] + ext for entry in HAIR_CATALOG for ext in [".png", ".mesh"]]:
                self.assertEqual((dest / name).read_bytes(), (DEST / name).read_bytes())

    def test_rejects_interpolated_sampler_and_unsupported_material(self):
        for name in ["player"] + [entry[2] for entry in HAIR_CATALOG]:
            glb = Glb(DEST / f"source/{name}.glb")
            glb.validate_material()
            glb.doc["samplers"][0]["magFilter"] = 9729
            with self.assertRaisesRegex(ValueError, "nearest clamp"):
                glb.validate_material()
            glb = Glb(DEST / f"source/{name}.glb")
            glb.doc["materials"][0]["alphaMode"] = "BLEND"
            with self.assertRaisesRegex(ValueError, "opaque"):
                glb.validate_material()

    def test_face_catalog_names_ids_and_approved_bytes(self):
        face = DEST / "face"
        mapping = json.loads((face / "mapping.json").read_text())
        expected = {
            "eyes": ["classic", "cute_glint", "kawaii_star", "playful_wink", "happy_crescent", "neon_focus", "neon_curious", "soft_sleepy"],
            "mouths": ["classic", "soft_smile", "cat_smile", "tiny_open", "playful", "smirk"],
        }
        for group, names in expected.items():
            self.assertEqual([entry["id"] for entry in mapping[group]], list(range(len(names))))
            self.assertEqual([entry["name"] for entry in mapping[group]], names)
        self.assertEqual(len(mapping["sha256"]), 23)
        for name, expected_hash in mapping["sha256"].items():
            data = (face / name).read_bytes()
            self.assertEqual(hashlib.sha256(data).hexdigest(), expected_hash)
            self.assertEqual(data[:8], b"\x89PNG\r\n\x1a\n")
            self.assertEqual(struct.unpack_from(">II", data, 16), (32, 32))
            self.assertEqual(data[24:26], bytes([8, 6]), "8-bit RGBA required")

    def test_contiguous_single_material_ranges_and_socket_budgets(self):
        validate_geometry(self.asset["vertices"], self.asset["indices"])
        mixed = self.asset["indices"].copy()
        mixed[0] = 216
        with self.assertRaisesRegex(ValueError, "crosses materials"):
            validate_geometry(self.asset["vertices"], mixed)
        disjoint = self.asset["indices"] + self.asset["indices"][:3]
        with self.assertRaisesRegex(ValueError, "contiguous and ordered"):
            validate_geometry(self.asset["vertices"], disjoint)
        outside = copy.deepcopy(self.asset["vertices"])
        next(v for v in outside if v["material"] == 3)["position"][1] = 0.82
        with self.assertRaisesRegex(ValueError, "socket envelope"):
            validate_geometry(outside, self.asset["indices"])

    def test_clearance_evidence_matches_exact_source_geometry_and_animations(self):
        evidence = json.loads((DEST / "hair_compatibility.json").read_text())
        self.assertEqual(evidence["geometry_animation_sha256"], geometry_animation_sha256(Glb(DEST / "source/player.glb")))
        sockets = json.loads((DEST / "hair_sockets.json").read_text())
        self.assertEqual([s["id"] for s in sockets["styles"]], list(range(1, 14)))
        self.assertEqual([s["key"] for s in sockets["styles"]], [e[1] for e in HAIR_CATALOG])
        for style in sockets["styles"]:
            self.assertEqual(hashlib.sha256((DEST / style["source"]).read_bytes()).hexdigest(), style["source_sha256"])
            self.assertEqual(hashlib.sha256((DEST / style["mesh"]).read_bytes()).hexdigest(), style["mesh_sha256"])
            lo, hi = style["index_range"]
            self.assertTrue(all(self.asset["vertices"][i]["material"] == style["id"] for i in self.asset["indices"][lo:hi]))
            self.assertIn(style["key"], evidence["styles"])
            self.assertEqual(style["source_sha256"], evidence["styles"][style["key"]]["source_sha256"])
            self.assertEqual(len(evidence["styles"][style["key"]]["clips"]), 5)

if __name__ == "__main__":
    unittest.main()
