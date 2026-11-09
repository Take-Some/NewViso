"""Validate shared door geometry/material closure against retained authoring inputs."""
from pathlib import Path
import hashlib,json,math,struct,zlib
ROOT=Path(__file__).resolve().parents[2]
SOURCE=ROOT/'Shared/Source/models/props/doors';CONTENT=ROOT/'Shared/Content'
def read(path):
 raw=path.read_bytes();assert struct.unpack_from('<H',raw,6)[0]==6,'door must use drawable content routing';n=struct.unpack_from('<Q',raw,16)[0];body=zlib.decompress(raw[-n:],-15)
 assert body[:4]==b'NVRM';k=struct.unpack_from('<I',body,8)[0]
 return json.loads(body[20:20+k]),body[20+k:]
def vectors(mesh,data,semantic,width):
 st=next(s for s in mesh['streams']if s['semantic']==semantic)
 return [struct.unpack_from('<'+'f'*width,data,st['offset']+i*st['stride'])for i in range(mesh['vertex_count'])]
def main():
 original,old=read(SOURCE/'office_panel.source.ydr')
 variants=[]
 for side,name in [(-1,'office_panel'),(1,'office_panel_right')]:
  model,data=read(CONTENT/('models/props/doors/'+name+'.ydr'))
  assert model['name']==name,'each hinge variant requires a distinct model identity'
  assert model['material_slots']==original['material_slots']
  vertices=0
  for src,mesh in zip(original['meshes'],model['meshes']):
   assert src['primitives']==mesh['primitives'] and src['vertex_count']==mesh['vertex_count']
   assert vectors(src,old,'texcoord0',2)==vectors(mesh,data,'texcoord0',2)
   si,di=src['index_buffer'],mesh['index_buffer'];assert old[si['offset']:si['offset']+si['length']]==data[di['offset']:di['offset']+di['length']]
   for n,t in zip(vectors(mesh,data,'normal',3),vectors(mesh,data,'tangent',4)):
    assert all(math.isfinite(x)for x in (*n,*t))
    assert abs(sum(n[i]*t[i]for i in range(3)))<2e-4
    assert abs(sum(x*x for x in n)-1)<2e-4
   vertices+=mesh['vertex_count']
  refs=[t['asset_ref']for m in model['material_slots']for t in m['textures']]
  assert all((CONTENT/ref.split('@')[0]).is_file()for ref in refs)
  variants.append((model,data))
 left,ld=variants[0];right,rd=variants[1]
 for lm,rm in zip(left['meshes'],right['meshes']):
  for a,b in zip(vectors(lm,ld,'position',3),vectors(rm,rd,'position',3)):
   assert max(abs(a[i]-b[i]*[-1,1,-1][i])for i in range(3))<1e-6
 report={'passed':True,'variants':2,'vertices_per_variant':vertices,'material_slots':len(original['material_slots']),'uv_topology_preserved':True,'finite_orthogonal_tangent_basis':True,'left_right_handles':True,'texture_files':list(set(ref.split('@')[0]for ref in refs))}
 path=ROOT/'NewViso/.newviso/cache/door-mechanisms-work/20261027/model-validation.json';path.write_text(json.dumps(report,indent=2)+'\n',encoding='utf8')
 print(json.dumps(report))
if __name__=='__main__':main()
