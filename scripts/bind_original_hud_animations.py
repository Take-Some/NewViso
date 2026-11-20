#!/usr/bin/env python3
"""Bind original HUD Ink animation tracks to compiled source widget paths."""
from pathlib import Path
import json,collections
ROOT=Path(__file__).resolve().parents[2]
P=ROOT/'Projects/KaylaNightCity'
BASE=P/'ui/ink/generated/hud'
LIB=json.loads((BASE/'animations/hud_source_animations_manifest.json').read_text('utf8'))
by_depot={item['source']:item for item in LIB['items'] if item.get('status')=='exported'}
report=[];totals=collections.Counter()
for path in sorted(BASE.glob('*.surface.json')):
 surface=json.loads(path.read_text('utf8'))
 ref=surface['metrics'].get('ink_source_animation')
 if not ref:
  totals['no_animation_ref']+=1;continue
 item=by_depot.get(ref)
 if not item:
  totals['missing_library']+=1;continue
 source=json.loads((P/item['asset']).read_text('utf8'))
 paths=collections.defaultdict(list)
 for n in surface['components']:
  target=n.get('props',{}).get('ink_animation_target_path')
  if target is not None:paths[target].append(n['id'])
 bindings={};bound=0;unbound=0
 for clip in source['clips']:
  entries={}
  for track in clip['tracks']:
   key='/'.join(str(i) for i in (track.get('target_path') or []))
   if key in paths:
    entries[key]=paths[key][0] if len(paths[key])==1 else paths[key]
    bound+=1
   else:unbound+=1
  bindings[clip['name']]=entries
 surface['metrics']['ink_animation_source']={
  'asset':item['asset'],'clips':[c['name'] for c in source['clips']],
  'bindings':bindings,'bound_tracks':bound,'unbound_tracks':unbound,
  'requires_gameplay_trigger':True}
 path.write_text(json.dumps(surface,ensure_ascii=False,indent=2)+'\n','utf8')
 totals.update(linked_library=1,bound_tracks=bound,unbound_tracks=unbound)
 report.append({'widget':path.stem,'source_animation':ref,
                'bound':bound,'unbound':unbound})
record={'schema':'newviso.ink.hud_animation_bindings.v1',
 'counts':dict(totals),'widgets':report,'verified_gameplay_event_bindings':False}
(BASE/'hud_animation_binding_manifest.json').write_text(
 json.dumps(record,ensure_ascii=False,indent=2)+'\n','utf8')
print(json.dumps(record['counts'],ensure_ascii=False))
print('BEST',sorted(report,key=lambda x:-x['bound'])[:7])
