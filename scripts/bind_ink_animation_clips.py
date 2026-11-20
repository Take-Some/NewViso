#!/usr/bin/env python3
"""Bind extracted Ink clips to preview screens by exact original depot paths."""
from pathlib import Path
import json
import importlib.util
from collections import Counter
ROOT=Path(__file__).resolve().parents[2]
P=ROOT/'Projects/KaylaNightCity'
def norm(x):
 s=str(x).replace(chr(92),'/').lower()
 for name in ('base/','ep1/','engine/'):
  i=s.find(name)
  if i>=0:return s[i:]
 return s
def scalar(x):
 if isinstance(x,dict):return scalar(x.get('$value',x.get('DepotPath')))
 return x
def recover_paths(src,surface):
 def load(file,name):
  spec=importlib.util.spec_from_file_location(name,ROOT/'NewViso/scripts'/file)
  m=importlib.util.module_from_spec(spec)
  spec.loader.exec_module(m)
  return m
 imp=load('import_redengine_ink.py','ink_bind_imp')
 cmp=load('build_ink_runtime_surfaces.py','ink_bind_cmp')
 tree,_=imp.InkImporter(imp.load_resource(src)).convert_surface('Root','ink.bound')
 nodes,_,_=cmp.relative_source_nodes(tree['components'][0])
 lookup={n['id']:n.get('props',{}).get('ink_animation_target_path') for n in nodes}
 for node in surface['components']:
  path=lookup.get(node['id'])
  if path:node['props']['ink_animation_target_path']=path

def main():
 base=P/'ui/ink/generated'
 m=json.loads((base/'animations/source_animation_manifest.json').read_text('utf-8'))
 assets={norm(r['depot']):r['asset'] for r in m['libraries']
         if r.get('status')=='source_curves_not_bound'}
 counts=Counter()
 results=[]
 for f in sorted(base.glob('*.surface.json')):
  name=f.name.removesuffix('.surface.json')
  raw=P/'.newviso/cache/ink_full_ui_decoded/fullscreen'/(name+'.inkwidget.json')
  if not raw.exists():counts['no_widget']+=1;continue
  original=json.loads(raw.read_text('utf-8'))
  ref=scalar(original['Data']['RootChunk'].get('animationLibraryResRef'))
  asset=assets.get(norm(ref)) if ref and ref!='0' else None
  if not asset:counts['no_matching_animation']+=1;continue
  surface=json.loads(f.read_text('utf-8'))
  if surface.get('metrics',{}).get('ink_animation'):
   counts['already_bound']+=1;continue
  data=json.loads((P/asset).read_text('utf-8'))
  recover_paths(raw,surface)
  paths={v.get('props',{}).get('ink_animation_target_path'):v['id']
         for v in surface.get('components',[]) if v.get('props',{}).get('ink_animation_target_path')}
  bindings={}
  matched=missing=0
  for clip in data['clips']:
   mapping={}
   for track in clip['tracks']:
    key='/'.join(map(str,track.get('target_path',[])))
    if key in paths:mapping[key]=paths[key];matched+=1
    else:missing+=1
   bindings[clip['name']]=mapping
  if matched==0:counts['no_bound_nodes']+=1;continue
  names={x['name'] for x in data['clips']}
  aliases={k:v for k,v in [('intro','intro'),('loop','loop'),
             ('hover_enter','pause_button_hover_over_anim'),
             ('hover_leave','pause_button_hover_out_anim')] if v in names}
  surface['metrics']['ink_animation']={'asset':asset,'clips':aliases,'bindings':bindings}
  surface['metrics']['ink_animation_binding_stats']={
    'bound_tracks':matched,'unbound_tracks':missing,'clips':len(names)}
  f.write_text(json.dumps(surface,ensure_ascii=False,indent=2)+'\n','utf-8')
  results.append({'screen':name,'asset':asset,'bound':matched,'unbound':missing})
  counts['bound']+=1
 report={'schema':'newviso.ink.animation_bindings.v1','counts':dict(counts),
         'bound':results,'full_effect_parity':False}
 (base/'source_animation_bindings.json').write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n','utf-8')
 print(dict(counts))
if __name__=='__main__':main()
