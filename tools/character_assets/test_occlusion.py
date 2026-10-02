"""Regression tests for pose-stable, bounded offline character visibility."""
import hashlib
import json
import struct
import unittest
from bake_occlusion import BIAS, DEST, MIN_VISIBILITY, SIDECAR, Vertex, bake_visibility, read_mesh


def scene(height=0.02, joint=5, material=1):
    # Receiver at origin, a broad blocker above it. Different joint-local
    # meshes can overlap numerically but must never occlude each other.
    vertices = [Vertex((0, 0, 0), (0, 1, 0), 5, 1)]
    vertices += [Vertex(point, (0, -1, 0), joint, material) for point in
                 [(-1, height, -1), (1, height, -1), (1, height, 1), (-1, height, 1)]]
    return vertices, [(1, 2, 3), (1, 3, 4)]


class OcclusionTests(unittest.TestCase):
    def test_nearby_same_joint_occludes_but_different_joint_or_material_does_not(self):
        self.assertLess(bake_visibility(*scene())[0], 230)
        self.assertEqual(bake_visibility(*scene(joint=4))[0], 255)
        self.assertEqual(bake_visibility(*scene(material=2))[0], 255)

    def test_contacts_fade_out_and_distant_or_coplanar_geometry_is_unoccluded(self):
        near = bake_visibility(*scene(height=0.01))[0]
        middle = bake_visibility(*scene(height=0.05))[0]
        self.assertGreater(middle, near)
        self.assertEqual(bake_visibility(*scene(height=0.101 + BIAS))[0], 255)
        self.assertEqual(bake_visibility(*scene(height=0))[0], 255)
        self.assertGreaterEqual(bake_visibility(*scene(height=BIAS * 2))[0], MIN_VISIBILITY)

    def test_deterministic_duplicate_vertices_and_triangle_order(self):
        vertices, triangles = scene()
        vertices.append(vertices[0])
        values = bake_visibility(vertices, triangles)
        self.assertEqual(values[0], values[-1])
        self.assertEqual(values, bake_visibility(vertices, list(reversed(triangles))))
        self.assertEqual(values, bake_visibility(vertices, triangles))

    def test_face_features_stay_open_and_mixed_rigid_triangles_are_rejected(self):
        vertices, triangles = scene()
        for surface in range(2, 7):
            vertices[0] = Vertex((0, 0, 0), (0, 1, 0), 5, 1, surface)
            self.assertEqual(bake_visibility(vertices, triangles)[0], 255)
        vertices[1] = Vertex(vertices[1].position, (0, -1, 0), 4, 1)
        with self.assertRaises(ValueError):
            bake_visibility(vertices, triangles)

    def test_checked_in_bake_hashes_and_vertex_counts_match_native_meshes(self):
        data = (DEST / SIDECAR).read_bytes()
        magic, count = struct.unpack_from("<4sI", data)
        self.assertEqual((magic, count), (b"BGA1", 15))
        offset = 8
        records = sorted(json.loads((DEST / "manifest.json").read_text())["records"], key=lambda r: r["id"])
        for record in records:
            digest, count = struct.unpack_from("<32sI", data, offset)
            mesh = (DEST / f'{record["key"]}.mesh').read_bytes()
            vertices, _ = read_mesh(mesh)
            self.assertEqual(digest, hashlib.sha256(mesh).digest())
            self.assertEqual(count, len(vertices))
            offset += 36
            values = data[offset:offset + count]
            self.assertEqual(len(values), count)
            self.assertTrue(all(MIN_VISIBILITY <= v <= 255 for v in values))
            offset += count
        self.assertEqual(offset, len(data))


if __name__ == "__main__":
    unittest.main()
