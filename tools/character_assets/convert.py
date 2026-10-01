#!/usr/bin/env python3
"""Convert the checked-in, rigid authored GLBs to a small native character asset.

No runtime importer or third-party Python package is required. Unsupported glTF
features fail explicitly rather than silently changing the authored model.
"""
import hashlib
import json
import math
from pathlib import Path
import struct

ROOT = Path(__file__).resolve().parents[2]
DEST = ROOT / "assets/models/player"
IDENTITY = [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1]
# Append-only appearance/material IDs, source/output stem, vertex budget and
# authored head-local bounding envelope in meters. ID 0 is body/no hair.
HAIR_CATALOG = [
    (1, "tousled_crop", "hair", 512, (-0.30, 0.18, -0.31), (0.30, 0.61, 0.30)),
    (2, "side_swept_undercut", "hair_undercut", 768, (-0.35, 0.15, -0.33), (0.29, 0.64, 0.30)),
    (3, "space_buns", "hair_space_buns", 1024, (-0.42, 0.20, -0.31), (0.42, 0.81, 0.31)),
    (4, "curly_bob", "hair_curly_bob", 3072, (-0.40, 0.11, -0.37), (0.40, 0.64, 0.40)),
    (5, "curly_pigtails", "hair_curly_pigtails", 2048, (-0.50, 0.11, -0.33), (0.50, 0.56, 0.38)),
    (6, 'sidepart_bob', 'hair_sidepart_bob', 512, (-0.3, 0.08, -0.3), (0.3, 0.56, 0.3)),
    (7, 'compact_braid', 'hair_compact_braid', 512, (-0.28, 0.08, -0.29), (0.28, 0.54, 0.4)),
    (8, 'long_loose_curls', 'hair_long_loose_curls', 2816, (-0.36, -0.47, -0.32), (0.36, 0.61, 0.65)),
    (9, 'long_curly_ponytail', 'hair_long_curly_ponytail', 2048, (-0.34, -0.49, -0.32), (0.34, 0.61, 0.82)),
    (10, 'half_up_curly_cascade', 'hair_half_up_curly_cascade', 2560, (-0.31, -0.41, -0.32), (0.31, 0.65, 0.67)),
    (11, 'rounded_afro', 'hair_rounded_afro', 4352, (-0.51, 0.21, -0.39), (0.5, 0.91, 0.47)),
    (12, 'twin_braids', 'hair_twin_braids', 3328, (-0.4, -0.54, -0.26), (0.4, 0.57, 0.41)),
    (13, 'curly_mohawk', 'hair_curly_mohawk', 1024, (-0.26, 0.34, -0.31), (0.26, 0.78, 0.34)),
]
MAX_TOTAL_VERTICES = 32768
MAX_TOTAL_INDICES = 98304


def require(condition, message):
    if not condition:
        raise ValueError(message)


class Glb:
    def __init__(self, path):
        data = path.read_bytes()
        require(len(data) <= 4 * 1024 * 1024, "GLB exceeds converter limit")
        require(struct.unpack_from("<III", data) == (0x46546C67, 2, len(data)), "invalid GLB header")
        offset, chunks = 12, {}
        while offset < len(data):
            length, kind = struct.unpack_from("<II", data, offset)
            offset += 8
            require(offset + length <= len(data) and kind not in chunks, "invalid GLB chunk")
            chunks[kind] = data[offset:offset + length]
            offset += length
        self.doc = json.loads(chunks[0x4E4F534A])
        self.binary = chunks[0x004E4942]
        require(not self.doc.get("extensionsRequired"), "required extensions unsupported")
        require(len(self.doc["buffers"]) == 1 and "uri" not in self.doc["buffers"][0], "embedded buffer required")

    def view(self, index):
        view = self.doc["bufferViews"][index]
        require(view.get("buffer", 0) == 0, "external buffer unsupported")
        offset, size = view.get("byteOffset", 0), view["byteLength"]
        require(offset + size <= len(self.binary), "buffer view out of bounds")
        return self.binary[offset:offset + size]

    def accessor(self, index):
        a = self.doc["accessors"][index]
        require(not a.get("sparse") and not a.get("normalized"), "sparse/normalized accessors unsupported")
        kind = {5126: "f", 5125: "I", 5123: "H", 5121: "B"}[a["componentType"]]
        width = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4, "MAT4": 16}[a["type"]]
        fmt = "<" + kind * width
        size = struct.calcsize(fmt)
        view = self.doc["bufferViews"][a["bufferView"]]
        stride = view.get("byteStride", size)
        data = self.view(a["bufferView"])
        offset = a.get("byteOffset", 0)
        require(stride >= size and a["count"] <= 65536, "invalid accessor bounds")
        require(offset + max(0, a["count"] - 1) * stride + size <= len(data), "accessor out of bounds")
        values = [list(struct.unpack_from(fmt, data, offset + i * stride)) for i in range(a["count"])]
        require(all(math.isfinite(x) for row in values for x in row), "nonfinite accessor")
        return values

    def validate_material(self):
        require(len(self.doc.get("materials", [])) == 1, "one source material required")
        material = self.doc["materials"][0]
        pbr = material.get("pbrMetallicRoughness", {})
        require(pbr.get("baseColorTexture", {}).get("index") == 0, "base color texture zero required")
        require(material.get("alphaMode", "OPAQUE") == "OPAQUE", "only opaque source materials supported")
        require(not material.get("emissiveTexture") and material.get("emissiveFactor", [0, 0, 0]) == [0, 0, 0], "emissive materials unsupported")
        require(self.doc.get("textures") == [{"sampler": 0, "source": 0}], "one embedded texture required")
        require(self.doc.get("samplers") == [{"magFilter": 9728, "minFilter": 9728, "wrapS": 33071, "wrapT": 33071}], "nearest clamp sampler required")

    def png(self):
        image, = self.doc["images"]
        require(image["mimeType"] == "image/png", "PNG texture required")
        data = self.view(image["bufferView"])
        require(data.startswith(b"\x89PNG\r\n\x1a\n"), "invalid PNG")
        return data


def transform(matrix, vector, w):
    return [sum(matrix[column * 4 + row] * vector[column] for column in range(3)) + matrix[12 + row] * w for row in range(3)]


def geometry_animation_sha256(glb):
    # Texture-independent fingerprint binds clearance evidence to the exact
    # geometry, rig and animation values, even when the face atlas is replaced.
    payload = {key: glb.doc[key] for key in ["meshes", "nodes", "skins", "animations"]}
    payload["accessor_values"] = [glb.accessor(i) for i in range(len(glb.doc["accessors"]))]
    return hashlib.sha256(json.dumps(payload, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


def validate_geometry(vertices, indices):
    require(0 < len(vertices) <= MAX_TOTAL_VERTICES, "total vertex budget exceeded")
    require(0 < len(indices) <= MAX_TOTAL_INDICES and len(indices) % 3 == 0, "total index budget exceeded")
    require(all(0 <= i < len(vertices) for i in indices), "triangle index out of bounds")
    require(all(v["material"] in range(len(HAIR_CATALOG) + 1) for v in vertices), "unknown material")
    materials = []
    for start in range(0, len(indices), 3):
        triangle = [vertices[i]["material"] for i in indices[start:start + 3]]
        require(len(set(triangle)) == 1, "triangle crosses materials")
        if not materials or materials[-1] != triangle[0]:
            materials.append(triangle[0])
    require(materials == list(range(len(HAIR_CATALOG) + 1)), "material ranges must be contiguous and ordered")
    require(sum(v["material"] == 0 for v in vertices) <= 256, "body vertex budget exceeded")
    for material, _, _, limit, lo, hi in HAIR_CATALOG:
        selected = [v for v in vertices if v["material"] == material]
        require(0 < len(selected) <= limit, "hair vertex budget exceeded")
        require(sum(vertices[i]["material"] == material for i in indices) <= limit * 3, "hair index budget exceeded")
        require(all(v["joint"] == 1 and all(lo[a] <= v["position"][a] <= hi[a] for a in range(3)) for v in selected), "hair outside authored head socket envelope")


def load_native(dest=DEST):
    """Reconstruct the exact runtime arrays for offline validation."""
    dest = Path(dest)
    result = json.loads((dest / "character.json").read_text())
    result["clips"] = [json.loads((dest / f"clip_{name}.json").read_text()) for name in ["walk", "idle", "crouch", "tool_use_left", "tool_use_right"]]
    for material, _, stem, _, _, _ in HAIR_CATALOG:
        data = (dest / f"{stem}.mesh").read_bytes()
        magic, count, index_count = struct.unpack_from("<4sII", data)
        require(magic == b"BGH1" and len(data) == 12 + count * 32 + index_count * 2, "invalid native hair mesh")
        start = len(result["vertices"])
        for i in range(count):
            values = struct.unpack_from("<8f", data, 12 + i * 32)
            result["vertices"].append({"position": list(values[:3]), "normal": list(values[3:6]), "uv": list(values[6:]), "joint": 1, "material": material})
        result["indices"].extend(start + struct.unpack_from("<H", data, 12 + count * 32 + i * 2)[0] for i in range(index_count))
    return result


def convert(dest=DEST):
    dest = Path(dest)
    body = Glb(dest / "source/player.glb")
    hairs = [Glb(dest / f"source/{entry[2]}.glb") for entry in HAIR_CATALOG]
    sources = [(body, 0)] + [(glb, entry[0]) for glb, entry in zip(hairs, HAIR_CATALOG)]
    for glb, _ in sources:
        glb.validate_material()
    skin, = body.doc["skins"]
    ids = skin["joints"]
    require(len(ids) == 7, "expected seven authored joints")
    bind = body.accessor(skin["inverseBindMatrices"])
    require(len(bind) == 7, "expected seven inverse bind matrices")
    for glb in hairs:
        require(len(glb.doc["nodes"]) == 1 and not glb.doc["nodes"][0].get("children"), "hair must be one identity socket mesh")
    nodes = body.doc["nodes"]
    parents = {child: parent for parent, node in enumerate(nodes) for child in node.get("children", [])}
    joints = []
    for i, node_id in enumerate(ids):
        node = nodes[node_id]
        require("matrix" not in node and node.get("scale", [1, 1, 1]) == [1, 1, 1], "scaled joints unsupported")
        parent = ids.index(parents[node_id]) if node_id in parents else None
        require(parent is None or parent < i, "joints must be topologically ordered")
        joints.append({"name": node["name"], "parent": parent, "translation": node.get("translation", [0, 0, 0]), "rotation": node.get("rotation", [0, 0, 0, 1])})
    head = next(i for i, joint in enumerate(joints) if joint["name"] == "head")
    vertices, indices = [], []
    for glb, material in sources:
        for node in glb.doc["nodes"]:
            if "mesh" not in node:
                continue
            require(node.get("translation", [0, 0, 0]) == [0, 0, 0] and node.get("rotation", [0, 0, 0, 1]) == [0, 0, 0, 1] and node.get("scale", [1, 1, 1]) == [1, 1, 1] and "matrix" not in node, "mesh-node transforms unsupported")
            if material == 0:
                require(node.get("skin") == 0, "body mesh must use skin zero")
                require(not any(node is glb.doc["nodes"][child] for parent in glb.doc["nodes"] for child in parent.get("children", [])), "mesh parent transforms unsupported")
            for primitive in glb.doc["meshes"][node["mesh"]]["primitives"]:
                require(primitive.get("mode", 4) == 4 and not primitive.get("targets"), "only rigid triangles supported")
                require(primitive.get("material", 0) == 0, "only source material zero supported")
                attrs = primitive["attributes"]
                positions, normals, uvs = [glb.accessor(attrs[name]) for name in ["POSITION", "NORMAL", "TEXCOORD_0"]]
                require(len(positions) == len(normals) == len(uvs), "attribute length mismatch")
                if material == 0:
                    joint_ids, weights = glb.accessor(attrs["JOINTS_0"]), glb.accessor(attrs["WEIGHTS_0"])
                    require(len(joint_ids) == len(weights) == len(positions), "skin length mismatch")
                    require(all(w == [1, 0, 0, 0] for w in weights), "weighted deformation unsupported")
                else:
                    joint_ids = [[head] for _ in positions]
                start = len(vertices)
                for p, n, uv, js in zip(positions, normals, uvs, joint_ids):
                    joint = js[0]
                    require(0 <= joint < len(joints), "joint index out of range")
                    matrix = bind[joint] if material == 0 else IDENTITY
                    local, normal = transform(matrix, p, 1), transform(matrix, n, 0)
                    require(abs(sum(x * x for x in normal) - 1) < 0.001, "nonrigid inverse bind matrix")
                    vertices.append({"position": local, "normal": normal, "uv": uv, "joint": joint, "material": material})
                local_indices = [v[0] for v in glb.accessor(primitive["indices"])]
                require(len(local_indices) % 3 == 0 and all(0 <= i < len(positions) for i in local_indices), "invalid triangles")
                indices.extend(start + i for i in local_indices)
    clips = []
    for animation in body.doc["animations"]:
        channels = []
        for channel in animation["channels"]:
            target = channel["target"]
            require(target["path"] in ["translation", "rotation"], "unsupported animation target")
            sampler = animation["samplers"][channel["sampler"]]
            require(sampler.get("interpolation", "LINEAR") == "LINEAR", "only LINEAR animation supported")
            times = [v[0] for v in body.accessor(sampler["input"])]
            values = body.accessor(sampler["output"])
            require(len(times) == len(values) and times and times[0] >= 0 and all(b > a for a, b in zip(times, times[1:])), "invalid keyframe timeline")
            channels.append({"joint": ids.index(target["node"]), "path": target["path"], "times": times, "values": [v + [0] if len(v) == 3 else v for v in values]})
        clips.append({"name": animation["name"], "duration": max(c["times"][-1] for c in channels), "looping": animation.get("extras", {}).get("loop", False), "channels": channels})
    require({c["name"] for c in clips} == {"idle", "walk", "crouch", "tool_use_left", "tool_use_right"}, "unexpected clip set")
    validate_geometry(vertices, indices)
    body_vertices = [v for v in vertices if v["material"] == 0]
    body_indices = [i for i in indices if vertices[i]["material"] == 0]
    result = {"version": 1, "joints": joints, "vertices": body_vertices, "indices": body_indices, "clips": []}
    (dest / "character.json").write_text(json.dumps(result, separators=(",", ":"), allow_nan=False) + "\n")
    for clip in clips:
        (dest / f"clip_{clip['name']}.json").write_text(json.dumps(clip, separators=(",", ":"), allow_nan=False) + "\n")
    (dest / "body.png").write_bytes(body.png())
    styles = []
    for glb, (material, key, stem, limit, lo, hi) in zip(hairs, HAIR_CATALOG):
        png = glb.png()
        (dest / f"{stem}.png").write_bytes(png)
        selected = [v for v in vertices if v["material"] == material]
        locations = [i for i, index in enumerate(indices) if vertices[index]["material"] == material]
        first_vertex = next(i for i, v in enumerate(vertices) if v["material"] == material)
        local_indices = [indices[i] - first_vertex for i in locations]
        require(len(selected) <= 65535 and all(0 <= i < len(selected) for i in local_indices), "hair mesh index exceeds u16")
        packed = struct.pack("<4sII", b"BGH1", len(selected), len(local_indices))
        packed += b"".join(struct.pack("<8f", *v["position"], *v["normal"], *v["uv"]) for v in selected)
        packed += struct.pack("<" + "H" * len(local_indices), *local_indices)
        (dest / f"{stem}.mesh").write_bytes(packed)
        styles.append({"id": material, "key": key, "source": f"source/{stem}.glb", "texture": f"{stem}.png", "mesh": f"{stem}.mesh", "mesh_sha256": hashlib.sha256(packed).hexdigest(),
            "source_sha256": hashlib.sha256((dest / f"source/{stem}.glb").read_bytes()).hexdigest(),
            "texture_sha256": hashlib.sha256(png).hexdigest(), "texture_size": list(struct.unpack_from(">II", png, 16)),
            "vertex_count": len(selected), "index_range": [locations[0], locations[-1] + 1],
            "head_local_bounds": [[min(v["position"][axis] for v in selected) for axis in range(3)], [max(v["position"][axis] for v in selected) for axis in range(3)]],
            "vertex_limit": limit, "head_local_bounds_limit": [lo, hi]})
    sockets = {"version": 1, "material_zero": "body; appearance hair ID 0 attaches no mesh", "socket": {"joint": head, "name": "head", "translation": [0, 0, 0], "rotation_xyzw": [0, 0, 0, 1], "scale": [1, 1, 1]}, "source_basis": {"up": "+Y", "forward": "-Z", "units": "meters"}, "native_uniform_scale": 0.9, "rigid": True, "secondary_motion": False, "geometry_animation_sha256": geometry_animation_sha256(body), "joint_count": 7, "clip_names": [clip["name"] for clip in clips], "total_vertex_limit": MAX_TOTAL_VERTICES, "total_index_limit": MAX_TOTAL_INDICES, "styles": styles}
    (dest / "hair_sockets.json").write_text(json.dumps(sockets, indent=2) + "\n")
    print(f"Converted {len(vertices)} vertices, {len(indices)//3} triangles, {len(joints)} joints, {len(clips)} clips")


if __name__ == "__main__":
    convert()
