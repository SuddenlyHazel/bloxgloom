#!/usr/bin/env python3
"""Bake bounded, pose-stable local visibility without modifying meshes or pixels.

Only triangles on the SAME rigid joint and material can occlude a vertex.
This deliberately excludes moving-limb/body contacts and other hair styles.
Rebuild: python3 tools/character_assets/bake_occlusion.py
Verify:  python3 tools/character_assets/bake_occlusion.py --check
"""
import argparse
from collections import defaultdict
from dataclasses import dataclass
import hashlib
import itertools
import json
import math
from pathlib import Path
import struct

ROOT = Path(__file__).resolve().parents[2]
DEST = ROOT / "assets/models/player/articulated"
SIDECAR = "local_visibility.ao"
RADIUS = 0.10  # metres, in the authored joint-local coordinate system
STRENGTH = 0.25  # at most 25% attenuation of indirect light, never direct light
SAMPLES = 32
BIAS = 0.001  # avoid adjacent coplanar/self-triangle intersections
MIN_VISIBILITY = 191
# Leave source-authored face features alone: fixed/eyes/iris/detail/brows.
RECEIVERS = frozenset((0, 1, 7, 8, 9))


@dataclass(frozen=True)
class Vertex:
    position: tuple
    normal: tuple
    joint: int
    material: int
    surface: int = 7


def sub(a, b):
    return tuple(x - y for x, y in zip(a, b))


def dot(a, b):
    return sum(x * y for x, y in zip(a, b))


def cross(a, b):
    return (a[1]*b[2] - a[2]*b[1], a[2]*b[0] - a[0]*b[2], a[0]*b[1] - a[1]*b[0])


def normalize(a):
    length = math.sqrt(dot(a, a))
    if not math.isfinite(length) or length < 1e-12:
        raise ValueError("invalid normal")
    return tuple(x / length for x in a)


def read_mesh(data):
    if len(data) < 12:
        raise ValueError("short mesh header")
    magic, count, index_count = struct.unpack_from("<4sII", data)
    if (magic != b"BGC2" or not 0 < count <= 16384
            or not 0 < index_count <= 49152 or index_count % 3
            or len(data) != 12 + count * 44 + index_count * 2):
        raise ValueError("invalid native mesh bounds")
    vertices = []
    for offset in range(12, 12 + count * 44, 44):
        values = struct.unpack_from("<8fIII", data, offset)
        if not all(math.isfinite(x) for x in values[:8]):
            raise ValueError("nonfinite vertex")
        vertices.append(Vertex(values[:3], normalize(values[3:6]), values[8], values[9], values[10]))
    indices = struct.unpack_from(f"<{index_count}H", data, 12 + count * 44)
    if max(indices) >= count:
        raise ValueError("out-of-bounds index")
    return vertices, [indices[i:i + 3] for i in range(0, index_count, 3)]


def hemisphere_directions(normal, samples=SAMPLES):
    """Deterministic cosine-weighted Hammersley hemisphere, without a seed."""
    helper = (0.0, 1.0, 0.0) if abs(normal[1]) < 0.9 else (1.0, 0.0, 0.0)
    tangent = normalize(cross(helper, normal))
    bitangent = cross(normal, tangent)
    for sample in range(samples):
        # Van der Corput radical inverse in base two.
        value, inverse, weight = sample, 0.0, 0.5
        while value:
            inverse += (value & 1) * weight
            value >>= 1
            weight *= 0.5
        radius = math.sqrt((sample + 0.5) / samples)
        angle = math.tau * inverse
        x, y, z = radius * math.cos(angle), radius * math.sin(angle), math.sqrt(1 - radius * radius)
        yield tuple(x * tangent[i] + y * bitangent[i] + z * normal[i] for i in range(3))


def ray_triangle(origin, direction, triangle, limit):
    """Two-sided Moller-Trumbore; return closest valid distance or the limit."""
    a, e1, e2, _, _ = triangle
    dx, dy, dz = direction
    px, py, pz = dy*e2[2] - dz*e2[1], dz*e2[0] - dx*e2[2], dx*e2[1] - dy*e2[0]
    determinant = e1[0]*px + e1[1]*py + e1[2]*pz
    if abs(determinant) < 1e-10:
        return limit
    inverse = 1.0 / determinant
    tx, ty, tz = origin[0] - a[0], origin[1] - a[1], origin[2] - a[2]
    u = (tx*px + ty*py + tz*pz) * inverse
    if u < -1e-7 or u > 1 + 1e-7:
        return limit
    qx, qy, qz = ty*e1[2] - tz*e1[1], tz*e1[0] - tx*e1[2], tx*e1[1] - ty*e1[0]
    v = (dx*qx + dy*qy + dz*qz) * inverse
    if v < -1e-7 or u + v > 1 + 1e-7:
        return limit
    distance = (e2[0]*qx + e2[1]*qy + e2[2]*qz) * inverse
    return distance if BIAS * 0.1 < distance < limit else limit


def bake_visibility(vertices, indices, radius=RADIUS, samples=SAMPLES):
    """Return one byte per vertex; no rest/world transform is ever used."""
    triangles = []
    bins = defaultdict(set)
    for indices3 in indices:
        corners = [vertices[i] for i in indices3]
        group = corners[0].joint, corners[0].material
        if any((v.joint, v.material) != group for v in corners):
            raise ValueError("triangle crosses a rigid joint or material")
        a, b, c = [v.position for v in corners]
        e1, e2 = sub(b, a), sub(c, a)
        if dot(cross(e1, e2), cross(e1, e2)) < 1e-20:
            continue
        low = tuple(min(p[axis] for p in (a, b, c)) for axis in range(3))
        high = tuple(max(p[axis] for p in (a, b, c)) for axis in range(3))
        triangle_id = len(triangles)
        triangles.append((a, e1, e2, low, high))
        ranges = [range(math.floor(low[i] / radius), math.floor(high[i] / radius) + 1) for i in range(3)]
        for cell in itertools.product(*ranges):
            bins[group, cell].add(triangle_id)
    values, cache = [], {}
    for vertex in vertices:
        if vertex.surface not in RECEIVERS:
            values.append(255)
            continue
        # UV seams and duplicate vertices with the same normal get identical AO.
        key = vertex.position, vertex.normal, vertex.joint, vertex.material
        if key in cache:
            values.append(cache[key])
            continue
        origin = tuple(vertex.position[i] + BIAS * vertex.normal[i] for i in range(3))
        cell = tuple(math.floor(x / radius) for x in origin)
        nearby = set()
        for delta in itertools.product((-1, 0, 1), repeat=3):
            neighbor = tuple(cell[i] + delta[i] for i in range(3))
            nearby.update(bins.get(((vertex.joint, vertex.material), neighbor), ()))
        candidates = []
        for triangle_id in sorted(nearby):
            triangle = triangles[triangle_id]
            low, high = triangle[3:]
            if sum(max(low[i] - origin[i], 0, origin[i] - high[i])**2 for i in range(3)) < radius**2:
                candidates.append(triangle)
        occlusion = 0.0
        for direction in hemisphere_directions(vertex.normal, samples):
            nearest = radius
            for triangle in candidates:
                nearest = ray_triangle(origin, direction, triangle, nearest)
            # Smooth compact support: contacts are strongest, reach vanishes at 10 cm.
            occlusion += (1.0 - nearest / radius)**2
        visibility = max(MIN_VISIBILITY, min(255, int(255 * (1 - STRENGTH * occlusion / samples) + 0.5)))
        cache[key] = visibility
        values.append(visibility)
    return bytes(values)


def build_sidecar(dest=DEST, report=False):
    records = json.loads((dest / "manifest.json").read_text())["records"]
    records.sort(key=lambda record: record["id"])
    if [r["id"] for r in records] != list(range(15)):
        raise ValueError("expected ordered builtin material records")
    output = bytearray(struct.pack("<4sI", b"BGA1", len(records)))
    for record in records:
        data = (dest / f'{record["key"]}.mesh').read_bytes()
        digest = hashlib.sha256(data).digest()
        if digest.hex() != record["mesh_sha256"]:
            raise ValueError(f'{record["key"]}: mesh does not match conversion manifest')
        vertices, indices = read_mesh(data)
        visibility = bake_visibility(vertices, indices)
        output.extend(struct.pack("<32sI", digest, len(visibility)))
        output.extend(visibility)
        if report:
            print(f'{record["key"]}: {len(visibility)} vertices, visibility {min(visibility) / 255:.3f}..1, '
                  f'mean {sum(visibility) / (255 * len(visibility)):.4f}', flush=True)
    return bytes(output)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="verify the checked-in bake is byte-identical")
    args = parser.parse_args()
    data = build_sidecar(report=True)
    path = DEST / SIDECAR
    if args.check:
        if path.read_bytes() != data:
            raise SystemExit("local visibility is stale; rerun bake_occlusion.py")
        print(f"Verified {len(data)} bytes of deterministic local visibility")
    else:
        path.write_bytes(data)
        print(f"Wrote {path.relative_to(ROOT)} ({len(data)} bytes)")


if __name__ == "__main__":
    main()
