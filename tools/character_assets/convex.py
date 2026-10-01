"""Offline convex SAT verification helpers; NumPy and SciPy required."""
import numpy as np
from scipy.spatial import ConvexHull

def unique_axes(a):
 a=np.asarray(a).reshape(-1,3);n=np.linalg.norm(a,axis=1);a=a[n>1e-8]/n[n>1e-8,None]
 # Sign-canonical, drop parallel duplicates; 1e-6 precision adequate for these meshes.
 sig=np.sign(a[np.arange(len(a)),np.argmax(abs(a),axis=1)]);a*=sig[:,None]
 return np.unique(np.round(a,7),axis=0)
class Poly:
 def __init__(self,name,pts,faces=None,joint=None):
  self.name=name;self.pts=np.unique(np.round(np.asarray(pts),8),axis=0);self.hull=ConvexHull(self.pts);self.norm=unique_axes(self.hull.equations[:,:3]);edges={}
  for i,tri in enumerate(self.hull.simplices):
   for j in range(3):edges.setdefault(tuple(sorted([tri[j],tri[(j+1)%3]])),[]).append(i)
  dirs=[]
  for (a,b),fs in edges.items():
   if len(fs)!=2 or abs(np.dot(self.hull.equations[fs[0],:3],self.hull.equations[fs[1],:3]))<1-1e-7:dirs.append(self.pts[a]-self.pts[b])
  self.edges=unique_axes(dirs);self.joint=joint;self.lo=self.pts.min(0);self.hi=self.pts.max(0)
  self.surface_convex=True
  if faces is not None:
   for f in faces:
    f=np.asarray(f);n=np.cross(f[1]-f[0],f[2]-f[0]);ln=np.linalg.norm(n)
    if ln<1e-9:continue
    d=(self.pts-f[0])@(n/ln)
    if d.min()<-1e-6 and d.max()>1e-6:self.surface_convex=False
 def transformed(self,M):
  q=object.__new__(Poly);q.name=self.name;q.pts=self.pts@M[:3,:3].T+M[:3,3];q.norm=self.norm@M[:3,:3].T;q.edges=self.edges@M[:3,:3].T;q.lo=q.pts.min(0);q.hi=q.pts.max(0);return q

def sat(a,b):
 """Exact separating axes for convex polyhedra; overlap depth is per-pair MTV."""
 axes=np.concatenate([a.norm,b.norm,unique_axes(np.cross(a.edges[:,None,:],b.edges[None,:,:]).reshape(-1,3))])
 aa=a.pts@axes.T;bb=b.pts@axes.T;amin=aa.min(0);amax=aa.max(0);bmin=bb.min(0);bmax=bb.max(0)
 sep=np.maximum(bmin-amax,amin-bmax)
 if sep.max()>1e-6:return None
 # For containment, intersection width is not translation distance. Two candidate displacement sides.
 left=amax-bmin;right=bmax-amin;dep=np.minimum(left,right);i=int(dep.argmin());v=-axes[i] if left[i]<right[i] else axes[i]
 return float(dep[i]),v
