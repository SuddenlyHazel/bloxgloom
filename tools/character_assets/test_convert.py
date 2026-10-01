"""Regression tests for the checked-in articulated conversion contract."""
import hashlib,json,math,shutil,struct,tempfile,unittest
from pathlib import Path
from articulated import BODIES,DEST,STYLES,convert,geometry,node_matrix,source_path
from glb import Glb

class ConversionTests(unittest.TestCase):
    def test_rebuild_is_byte_identical(self):
        with tempfile.TemporaryDirectory() as folder:
            output=Path(folder);shutil.copytree(DEST/'source',output/'source');convert(output)
            for file in DEST.iterdir():
                if file.suffix in ['.mesh','.png'] or file.suffix=='.json':
                    self.assertEqual(file.read_bytes(),(output/file.name).read_bytes(),file.name)
    def test_lossless_body_source_containers_are_deterministic_and_hash_original_bytes(self):
        import gzip
        manifest=json.loads((DEST/'manifest.json').read_text())
        for key in BODIES:
            path=source_path(DEST,key);self.assertEqual(path.suffix,'.gz')
            raw=Glb(path).raw_bytes
            self.assertEqual(gzip.compress(raw,compresslevel=9,mtime=0),path.read_bytes())
            record=next(r for r in manifest['records'] if r['key']==key)
            self.assertEqual(hashlib.sha256(raw).hexdigest(),record['source_sha256'])
    def test_bodies_share_rig_but_have_distinct_geometry(self):
        a=Glb(source_path(DEST,BODIES[0]));b=Glb(source_path(DEST,BODIES[1]))
        for aa,bb in zip(a.doc['nodes'][:30],b.doc['nodes'][:30]):
            self.assertEqual({k:v for k,v in aa.items() if k!='children'},{k:v for k,v in bb.items() if k!='children'})
            self.assertEqual([c for c in aa.get('children',[]) if c<30],[c for c in bb.get('children',[]) if c<30])
        self.assertEqual([len(a.doc['meshes']),len(b.doc['meshes'])],[51,62])
        self.assertNotEqual(geometry(a,0)[0],geometry(b,14)[0])
    def test_all_hair_has_head_joint_and_neutral_fixed_material_contract(self):
        for i,key in enumerate(STYLES,1):
            g=Glb(DEST/'source'/f'{key}.glb');packed,vertices,indices,parts=geometry(g,i)
            self.assertTrue(all(v['joint']==5 and v['material']==i and v['surface'] in [7,8] for v in vertices))
            self.assertEqual(packed,(DEST/f'{key}.mesh').read_bytes())
            for material in g.doc['materials']:
                pbr=material['pbrMetallicRoughness']
                if material['extras']['tintable']:
                    srgb=material['extras']['hairColor'];linear=[x/12.92 if x<=.04045 else ((x+.055)/1.055)**2.4 for x in srgb]
                    self.assertEqual(linear,pbr['baseColorFactor'][:3])
                else:self.assertNotIn('baseColorFactor',pbr)
            self.assertEqual(sum(p['count'] for p in parts),len(vertices))
    def test_native_header_sizes_and_hashes(self):
        records=json.loads((DEST/'manifest.json').read_text())['records'];self.assertEqual(len(records),15)
        for record in records:
            data=(DEST/f"{record['key']}.mesh").read_bytes();magic,n,ni=struct.unpack_from('<4sII',data)
            self.assertEqual(magic,b'BGC2');self.assertEqual(len(data),12+n*44+ni*2)
            self.assertEqual(hashlib.sha256(data).hexdigest(),record['mesh_sha256'])
            self.assertEqual(ni//3,record['triangles'])
    def test_source_clips_remain_exact_and_gameplay_is_not_old_rig_animation(self):
        rig=json.loads((DEST/'rig.json').read_text());source=Glb(source_path(DEST,'flat_chest'));rig['clips']=[json.loads((DEST/f"clip_{a['name']}.json").read_text()) for a in source.doc['animations']]
        self.assertEqual(len(rig['joints']),30);self.assertEqual(len(rig['clips']),4)
        self.assertEqual([c['name'] for c in rig['clips']],[a['name'] for a in source.doc['animations']])
        for clip,animation in zip(rig['clips'],source.doc['animations']):
            for track,ch in zip(clip['channels'],animation['channels']):
                sampler=animation['samplers'][ch['sampler']]
                times=[x[0] for x in source.accessor(sampler['input'])];values=source.accessor(sampler['output'])
                if all(x==values[0] for x in values):times=[times[0],times[-1]];values=[values[0],values[-1]]
                self.assertEqual(track['times'],times)
                self.assertEqual(track['values'],[x+[0] if len(x)==3 else x for x in values])
    def test_rigid_skin_and_node_bounds_reject_unsupported_inputs(self):
        for node in [{'matrix':[0]*16},{'scale':[-1,1,1]},{'translation':[math.nan,0,0]},{'rotation':[1,1,1,1]}]:
            with self.assertRaises(ValueError):node_matrix(node)
        g=Glb(source_path(DEST,'flat_chest'));g.doc['nodes'][30]['skin']=1
        with self.assertRaises(ValueError):geometry(g,0)
    def test_primitive_material_assignment_separates_overlapping_tie_uvs(self):
        _,v,_,_=geometry(Glb(DEST/'source'/'twin_braids.glb'),12)
        tint={tuple(x['uv']) for x in v if x['surface']==7};fixed={tuple(x['uv']) for x in v if x['surface']==8}
        self.assertTrue(tint and fixed and tint&fixed,'UV overlap needs per-primitive role, never UV-based tint classification')
if __name__=='__main__':unittest.main()
