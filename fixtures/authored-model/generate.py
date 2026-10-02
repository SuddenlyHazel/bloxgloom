"""Deterministic tiny GLB fixture for the native model preview (standard library)."""
import json
from pathlib import Path
import struct
import zlib

ROOT = Path(__file__).parent
data = bytearray()
doc = {'asset': {'version': '2.0', 'generator': 'Bloxgloom native model fixture'},
       'scene': 0, 'scenes': [{'nodes': [0]}], 'nodes': [], 'meshes': [],
       'bufferViews': [], 'accessors': [], 'materials': [], 'animations': []}


def view(raw):
    while len(data) % 4:
        data.append(0)
    i = len(doc['bufferViews'])
    doc['bufferViews'].append({'buffer': 0, 'byteOffset': len(data), 'byteLength': len(raw)})
    data.extend(raw)
    return i


def accessor(values, width, kind, bounds=False):
    flattened = [v for row in values for v in row]
    a = {'bufferView': view(struct.pack('<'+'f'*len(flattened), *flattened)),
         'componentType': 5126, 'count': len(values), 'type': kind}
    if bounds:
        a['min'] = [min(row[i] for row in values) for i in range(width)]
        a['max'] = [max(row[i] for row in values) for i in range(width)]
    i = len(doc['accessors'])
    doc['accessors'].append(a)
    return i


def chunk(kind, raw):
    return struct.pack('>I', len(raw))+kind+raw+struct.pack('>I', zlib.crc32(kind+raw))


png = b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR', struct.pack('>IIBBBBB', 2, 2, 8, 6, 0, 0, 0))
png += chunk(b'IDAT', zlib.compress(b'\0'+bytes([255,255,255,255,205,205,205,255])+b'\0'+bytes([205,205,205,255,255,255,255,255])))
png += chunk(b'IEND', b'')
doc['images'] = [{'bufferView': view(png), 'mimeType': 'image/png'}]
doc['textures'] = [{'source': 0, 'sampler': 0}]
doc['samplers'] = [{'magFilter': 9728, 'minFilter': 9728, 'wrapS': 33071, 'wrapT': 33071}]
for name, color in [('body', [1,1,1,1]), ('eye', [1,1,1,1]), ('fixed', [0.03,0.025,0.02,1])]:
    doc['materials'].append({'name': name, 'pbrMetallicRoughness': {
        'baseColorFactor': color, 'baseColorTexture': {'index': 0}, 'metallicFactor': 0}})


def mesh(quads, material):
    positions, normals, uvs = [], [], []
    for points, normal in quads:
        for i in [0,1,2,0,2,3]:
            positions.append(points[i]); normals.append(normal)
            uvs.append([(0,0),(1,0),(1,1),(0,1)][i])
    i = len(doc['meshes'])
    doc['meshes'].append({'primitives': [{'material': material, 'attributes': {
        'POSITION': accessor(positions, 3, 'VEC3', True),
        'NORMAL': accessor(normals, 3, 'VEC3'), 'TEXCOORD_0': accessor(uvs, 2, 'VEC2')}}]})
    return i


def cube(x, y, z):
    return [
        ([[-x,0,-z],[-x,y,-z],[x,y,-z],[x,0,-z]], [0,0,-1]),
        ([[x,0,z],[x,y,z],[-x,y,z],[-x,0,z]], [0,0,1]),
        ([[-x,0,z],[-x,y,z],[-x,y,-z],[-x,0,-z]], [-1,0,0]),
        ([[x,0,-z],[x,y,-z],[x,y,z],[x,0,z]], [1,0,0]),
        ([[-x,y,-z],[-x,y,z],[x,y,z],[x,y,-z]], [0,1,0]),
        ([[-x,0,z],[-x,0,-z],[x,0,-z],[x,0,z]], [0,-1,0])]


def rect(x, y, w, h):
    return ([[x,y,-0.306],[x,y+h,-0.306],[x+w,y+h,-0.306],[x+w,y,-0.306]], [0,0,-1])


doc['nodes'] = [
    {'name':'root','children':[1,2]},
    {'name':'body','mesh':mesh(cube(0.28,1.15,0.2),0)},
    {'name':'head','translation':[0,1.15,0],'mesh':mesh(cube(0.3,0.6,0.3),0),'children':[3,4,5,6]},
    {'name':'eyes_classic','mesh':mesh([rect(-0.2,0.25,0.1,0.12),rect(0.1,0.25,0.1,0.12)],1)},
    {'name':'eyes_sleepy','mesh':mesh([rect(-0.2,0.28,0.1,0.025),rect(0.1,0.28,0.1,0.025)],2)},
    {'name':'mouth','mesh':mesh([rect(-0.07,0.1,0.14,0.035)],2)},
    {'name':'hat','translation':[0,0.6,0],'mesh':mesh(cube(0.36,0.12,0.36),0)}]

for name, node, path, times, values in [
    ('idle', 2, 'rotation', [0,1,2,3], [[0,0,0,1],[0,0.05,0,(1-0.05**2)**0.5],[0,-0.05,0,(1-0.05**2)**0.5],[0,0,0,1]]),
    ('bounce', 0, 'translation', [0,0.25,0.5], [[0,0,0],[0,0.15,0],[0,0,0]]),
    ('nod', 2, 'rotation', [0,0.4,0.8], [[0,0,0,1],[0.2,0,0,(1-0.2**2)**0.5],[0,0,0,1]])]:
    width = len(values[0])
    doc['animations'].append({'name':name, 'samplers':[{
        'input':accessor([[t] for t in times],1,'SCALAR'),
        'output':accessor(values,width,f'VEC{width}'),'interpolation':'LINEAR'}],
        'channels':[{'sampler':0,'target':{'node':node,'path':path}}]})

doc['buffers'] = [{'byteLength':len(data)}]
encoded = json.dumps(doc, separators=(',', ':')).encode()
encoded += b' ' * ((-len(encoded)) % 4)
data.extend(b'\0' * ((-len(data)) % 4))
length = 12+8+len(encoded)+8+len(data)
glb = struct.pack('<IIIII',0x46546c67,2,length,len(encoded),0x4e4f534a)+encoded
glb += struct.pack('<II',len(data),0x004e4942)+data
(ROOT/'model.glb').write_bytes(glb)
print(f'Wrote {len(glb)} byte native model fixture')
