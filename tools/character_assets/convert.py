#!/usr/bin/env python3
"""Convert the checked-in, rigid authored GLBs to a small native character asset.

No runtime importer or third-party Python package is required. Unsupported glTF
features fail explicitly rather than silently changing the authored model.
"""
import json
import math
from pathlib import Path
import struct

ROOT = Path(__file__).resolve().parents[2]
DEST = ROOT / "assets/models/player"
IDENTITY = [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1]


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

    def png(self):
        image, = self.doc["images"]
        require(image["mimeType"] == "image/png", "PNG texture required")
        data = self.view(image["bufferView"])
        require(data.startswith(b"\x89PNG\r\n\x1a\n"), "invalid PNG")
        return data


def transform(matrix, vector, w):
    return [sum(matrix[column * 4 + row] * vector[column] for column in range(3)) + matrix[12 + row] * w for row in range(3)]


def convert():
    body = Glb(DEST / "source/player.glb")
    hair = Glb(DEST / "source/hair.glb")
    skin, = body.doc["skins"]
    ids = skin["joints"]
    require(len(ids) == 7, "expected seven authored joints")
    bind = body.accessor(skin["inverseBindMatrices"])
    require(len(bind) == 7, "expected seven inverse bind matrices")
    require(len(hair.doc["nodes"]) == 1 and not hair.doc["nodes"][0].get("children"), "hair must be one identity socket mesh")
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
    for glb, material in [(body, 0), (hair, 1)]:
        for node in glb.doc["nodes"]:
            if "mesh" not in node:
                continue
            require(node.get("translation", [0, 0, 0]) == [0, 0, 0] and node.get("rotation", [0, 0, 0, 1]) == [0, 0, 0, 1] and node.get("scale", [1, 1, 1]) == [1, 1, 1] and "matrix" not in node, "mesh-node transforms unsupported")
            if material == 0:
                require(node.get("skin") == 0, "body mesh must use skin zero")
                require(not any(node is glb.doc["nodes"][child] for parent in glb.doc["nodes"] for child in parent.get("children", [])), "mesh parent transforms unsupported")
            for primitive in glb.doc["meshes"][node["mesh"]]["primitives"]:
                require(primitive.get("mode", 4) == 4 and not primitive.get("targets"), "only rigid triangles supported")
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
    result = {"version": 1, "joints": joints, "vertices": vertices, "indices": indices, "clips": clips}
    (DEST / "character.json").write_text(json.dumps(result, separators=(",", ":"), allow_nan=False) + "\n")
    (DEST / "body.png").write_bytes(body.png())
    (DEST / "hair.png").write_bytes(hair.png())
    print(f"Converted {len(vertices)} vertices, {len(indices)//3} triangles, {len(joints)} joints, {len(clips)} clips")


if __name__ == "__main__":
    convert()
