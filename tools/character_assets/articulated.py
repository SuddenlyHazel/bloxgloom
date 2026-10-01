#!/usr/bin/env python3
"""Bounded offline conversion of the articulated, rigid 30-joint character kit.

Bake neutral hair adjustment handles at their validated rest positions. Keep
fixed accessory primitives separate from tintable hair, even with shared UVs.
No model parser, file path, or user-provided geometry enters the game runtime.
"""
import hashlib
import json
import math
from pathlib import Path
import struct
from glb import Glb, require, transform, IDENTITY

ROOT = Path(__file__).resolve().parents[2]
DEST = ROOT / 'assets/models/player/articulated'
STYLES = ['tousled_crop', 'side_swept_undercut', 'space_buns', 'curly_bob', 'curly_pigtails', 'sidepart_bob', 'compact_braid', 'long_loose_curls', 'long_curly_ponytail', 'half_up_curly_cascade', 'rounded_afro', 'twin_braids', 'curly_mohawk']
BODIES = ['flat_chest','defined_chest_sports_bra']
# Explicit semantic surface, independent of texture UV overlap.
SKIN, CLOTH, FIXED, WHITE, IRIS, DETAIL, BROW, HAIR, ACCESSORY, UPPER_CLOTH = range(10)

def source_path(dest, key):
    path=Path(dest)/'source'/f'{key}.glb'
    return path if path.exists() else path.with_suffix('.glb.gz')

def multiply(a,b):
    return [sum(a[k*4+r]*b[c*4+k] for k in range(4)) for c in range(4) for r in range(4)]

def node_matrix(node):
    require('matrix' not in node, 'matrix nodes unsupported')
    x,y,z,w=node.get('rotation',[0,0,0,1]); sx,sy,sz=node.get('scale',[1,1,1]);tx,ty,tz=node.get('translation',[0,0,0])
    require(all(math.isfinite(v) for v in [x,y,z,w,sx,sy,sz,tx,ty,tz]),'nonfinite node')
    require(abs(x*x+y*y+z*z+w*w-1)<.001 and min(sx,sy,sz)>0,'invalid node transform')
    return [(1-2*y*y-2*z*z)*sx,(2*x*y+2*z*w)*sx,(2*x*z-2*y*w)*sx,0,(2*x*y-2*z*w)*sy,(1-2*x*x-2*z*z)*sy,(2*y*z+2*x*w)*sy,0,(2*x*z+2*y*w)*sz,(2*y*z-2*x*w)*sz,(1-2*x*x-2*y*y)*sz,0,tx,ty,tz,1]

def surfaces(name):
    if name.startswith(('sports_bra','bra_')):return UPPER_CLOTH
    if name.startswith('shorts'):return CLOTH
    if name.startswith('eye_white'):return WHITE
    if name.startswith('iris'):return IRIS
    if name.startswith(('pupil','eye_glint')):return DETAIL
    if name.startswith('brow'):return BROW
    if name.startswith('eye_socket'):return FIXED
    return SKIN

def geometry(g,material):
    nodes=g.doc['nodes'];parents={c:p for p,n in enumerate(nodes) for c in n.get('children',[])}
    worlds={}
    def world(i):
        if i not in worlds:worlds[i]=multiply(world(parents[i]) if i in parents else IDENTITY,node_matrix(nodes[i]))
        return worlds[i]
    is_body=material in [0,14]
    bind=g.accessor(g.doc['skins'][0]['inverseBindMatrices']) if is_body else None
    vertices=[];indices=[];parts=[]
    for ni,node in enumerate(nodes):
        if 'mesh' not in node:continue
        first=len(vertices)
        for p in g.doc['meshes'][node['mesh']]['primitives']:
            require(p.get('mode',4)==4 and not p.get('targets'),'only rigid triangles supported')
            a=p['attributes'];pos,norm,uv=[g.accessor(a[k]) for k in ['POSITION','NORMAL','TEXCOORD_0']]
            require(len(pos)==len(norm)==len(uv),'attribute length mismatch')
            if is_body:
                require(node.get('skin')==0,'body skin zero required')
                js=g.accessor(a['JOINTS_0']);ws=g.accessor(a['WEIGHTS_0']);require(all(w==[1,0,0,0] for w in ws),'weighted deformation unsupported')
                surface=surfaces(node['name'])
            else:
                js=[[5] for _ in pos]
                mat=g.doc['materials'][p.get('material',0)]
                surface=HAIR if mat.get('extras',{}).get('tintable',False) else ACCESSORY
            start=len(vertices)
            for v,n,u,j in zip(pos,norm,uv,js):
                m=bind[j[0]] if is_body else world(ni)
                local=transform(m,v,1);normal=transform(m,n,0)
                length=math.sqrt(sum(x*x for x in normal));require(length>0,'zero normal');normal=[x/length for x in normal]
                require(j[0]<30 and all(math.isfinite(x) for x in local+normal+u),'invalid vertex')
                vertices.append(dict(position=local,normal=normal,uv=u,joint=j[0],material=material,surface=surface))
            ii=[i[0] for i in g.accessor(p['indices'])];require(len(ii)%3==0 and all(0<=i<len(pos) for i in ii),'invalid indices')
            indices.extend(start+i for i in ii)
        parts.append(dict(name=node['name'],first=first,count=len(vertices)-first))
    require(0<len(vertices)<=16384 and len(indices)<=49152,'mesh budget exceeded')
    require(all(-2<=x<=2 for v in vertices for x in v['position']),'mesh coordinate budget')
    packed=struct.pack('<4sII',b'BGC2',len(vertices),len(indices))
    packed+=b''.join(struct.pack('<8fIII',*v['position'],*v['normal'],*v['uv'],v['joint'],v['material'],v['surface']) for v in vertices)
    packed+=struct.pack('<'+'H'*len(indices),*indices)
    return packed,vertices,indices,parts

def convert(dest=DEST):
    dest=Path(dest);records=[];joints=None;clips=None
    for material,key in [(0,BODIES[0])]+list(enumerate(STYLES,1))+[(14,BODIES[1])]:
        path=source_path(dest,key);g=Glb(path)
        packed,v,ii,parts=geometry(g,material);(dest/f'{key}.mesh').write_bytes(packed)
        if material in [0,14]:
            skin,=g.doc['skins'];ids=skin['joints'];require(len(ids)==30,'30 joints required')
            parents={c:p for p,n in enumerate(g.doc['nodes']) for c in n.get('children',[])}
            new=[dict(name=g.doc['nodes'][i]['name'],parent=ids.index(parents[i]) if i in parents else None,translation=g.doc['nodes'][i].get('translation',[0,0,0]),rotation=g.doc['nodes'][i].get('rotation',[0,0,0,1])) for i in ids]
            require(all(j['parent'] is None or j['parent']<i for i,j in enumerate(new)),'joint hierarchy order')
            if joints is None:joints=new
            else:require(joints==new,'body variants must share rig')
            (dest/f'{key}.png').write_bytes(g.png())
            if clips is None:
                clips=[]
                for animation in g.doc['animations']:
                    channels=[]
                    for ch in animation['channels']:
                        target=ch['target'];s=animation['samplers'][ch['sampler']]
                        require(target['path'] in ['translation','rotation'] and s.get('interpolation','LINEAR')=='LINEAR','unsupported clip channel')
                        times=[x[0] for x in g.accessor(s['input'])];values=g.accessor(s['output'])
                        # Constant LINEAR tracks are exactly represented by two keys.
                        if len(times)>2 and all(v==values[0] for v in values):
                            times=[times[0],times[-1]];values=[values[0],values[-1]]
                        channels.append(dict(joint=ids.index(target['node']),path=target['path'],times=times,values=[x+[0] if len(x)==3 else x for x in values]))
                    clips.append(dict(name=animation['name'],duration=max(c['times'][-1] for c in channels),looping=animation.get('extras',{}).get('loop',False),channels=channels))
        else:
            for slot,role in [(0,'neutral'),(1,'accessory')]:
                tex=g.doc['textures'][min(slot,len(g.doc['textures'])-1)];image=g.doc['images'][tex['source']]
                require(image['mimeType']=='image/png','PNG required')
                (dest/f'{key}_{role}.png').write_bytes(g.view(image['bufferView']))
        records.append(dict(id=material,key=key,vertices=len(v),triangles=len(ii)//3,source_sha256=hashlib.sha256(g.raw_bytes).hexdigest(),mesh_sha256=hashlib.sha256(packed).hexdigest(),parts=parts,bounds=[[min(x['position'][a] for x in v) for a in range(3)],[max(x['position'][a] for x in v) for a in range(3)]]))
    (dest/'rig.json').write_text(json.dumps(dict(version=2,joints=joints,vertices=[],indices=[],clips=[]),separators=(',',':'))+'\n')
    for clip in clips:
        (dest/f"clip_{clip['name']}.json").write_text(json.dumps(clip,separators=(',',':'))+'\n')
    (dest/'manifest.json').write_text(json.dumps(dict(version=2,joints=30,source_height=1.8,head_joint=5,source_forward='-Z',runtime_forward='+Z',runtime_scale=1,records=records),indent=2)+'\n')
    print('Converted',sum(r['vertices'] for r in records),'vertices;',sum(r['triangles'] for r in records),'triangles; 30 joints; both bodies and 13 hairstyles')
if __name__=='__main__':convert()
