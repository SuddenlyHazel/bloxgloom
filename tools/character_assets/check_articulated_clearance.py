#!/usr/bin/env python3
"""Check the exact native vertex bytes against matrices exported by Rust tests.

Usage: BLOXGLOOM_POSE_DUMP=/tmp/poses.json cargo test native_articulated_pose_dump
       python tools/character_assets/check_articulated_clearance.py /tmp/poses.json out.json
Only scalp/head-shell contact is allowed. Nonconvex body surfaces use an outer
convex hull, so a reported overlap may be conservative and must be inspected.
"""
import hashlib,itertools,json,struct,sys,time
from pathlib import Path
import numpy as np
from convex import Poly,sat
from articulated import DEST

def polys(record):
    data=(DEST/f"{record['key']}.mesh").read_bytes();magic,n,ni=struct.unpack_from('<4sII',data);assert magic==b'BGC2'
    vertices=[];joints=[]
    for i in range(n):
        row=struct.unpack_from('<8fIII',data,12+i*44);vertices.append(row[:3]);joints.append(row[8])
    vertices=np.array(vertices);result=[]
    for part in record['parts']:
        a,b=part['first'],part['first']+part['count'];js=set(joints[a:b]);assert len(js)==1
        result.append(Poly(part['name'],vertices[a:b],joint=js.pop()))
    return result

def check(hair,body):
    lo=np.array([p.lo for p in body]);hi=np.array([p.hi for p in body]);hits=[]
    for a in hair:
        possible=np.flatnonzero(np.all(a.hi>=lo-1e-7,axis=1)&np.all(a.lo<=hi+1e-7,axis=1))
        for i in possible:
            b=body[i]
            if b.name=='head_shell':continue
            collision=sat(a,b)
            if collision and collision[0]>1e-6:hits.append(dict(hair=a.name,body=b.name,depth_m=collision[0]))
    return hits

def main():
    path=Path(sys.argv[1]);poses=json.loads(path.read_text());assert poses['matrix_layout']=='column_major'
    manifest=json.loads((DEST/'manifest.json').read_text());records=manifest['records']
    hairs={r['key']:polys(r) for r in records[1:14]};bodies={r['key']:polys(r) for r in [records[0],records[14]]}
    face=Poly('protected_face_below_brows',list(itertools.product([-.241,.241],[.006,.3065],[-1.25,-.19749375])))
    results={'schema':1,'method':'Native BGC2 rigid vertex bytes, Rust gameplay matrices, convex separating-axis theorem, face+ears protected; scalp head-shell contact excluded. Finite sampled grid, not continuous guarantee.','poses_sha256':hashlib.sha256(path.read_bytes()).hexdigest(),'samples':len(poses['samples']),'combinations':0,'collisions':[],'sources':{r['key']:r['mesh_sha256'] for r in records}}
    for index,sample in enumerate(poses['samples']):
        matrices=np.array(sample['matrices']).reshape(30,4,4).transpose(0,2,1);head_inv=np.linalg.inv(matrices[5])
        for name,body in bodies.items():
            transformed=[p.transformed(head_inv@matrices[p.joint]) for p in body]+[face]
            for style,hair in hairs.items():
                hits=check(hair,transformed);results['combinations']+=1
                if hits:results['collisions'].append(dict(pose=sample['name'],body=name,style=style,hits=hits))
        if index%100==0:
            print(index,'/',len(poses['samples']),'poses; overlapping combos',len(results['collisions']),flush=True)
            Path(sys.argv[2]).write_text(json.dumps(results,indent=2))
    results['passed']=not results['collisions'];Path(sys.argv[2]).write_text(json.dumps(results,indent=2));print('FINAL',results['combinations'],'combinations',len(results['collisions']),'overlapping')
    return 0 if results['passed'] else 1
if __name__=='__main__':raise SystemExit(main())
