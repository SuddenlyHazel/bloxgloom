"""Strict embedded GLB reader used only during offline conversion."""
import gzip,json,math,struct
IDENTITY = [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1]

def require(condition, message):
    if not condition:
        raise ValueError(message)


class Glb:
    def __init__(self, path):
        if path.suffix == ".gz":
            with gzip.open(path, "rb") as stream:
                data = stream.read(4 * 1024 * 1024 + 1)
        else:
            data = path.read_bytes()
        self.raw_bytes = data
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
