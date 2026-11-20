#!/usr/bin/env python3
"""Convert original HUD .inkanim JSON into NewViso source animation libraries."""
from pathlib import Path
import importlib.util,json,hashlib,collections
ROOT=Path(__file__).resolve().parents[2]
P=ROOT/'Projects/KaylaNightCity'
DECODE=P/'.newviso/cache/ink_full_ui_decoded/hud_animations'
EXPORT=P/'ui/ink/generated/hud/animations'
spec=importlib.util.spec_from_file_location('ink_import',ROOT/'NewViso/scripts/import_redengine_ink.py')
imp=importlib.util.module_from_spec(spec)
spec.loader.exec_module(imp)
catalog=json.loads((P/'ui/ink/generated/hud/source_hud_surfaces_manifest.json').read_text())
refs=sorted({x['source_animation'] for x in catalog['widgets'] if x.get('source_animation')})
EXPORT.mkdir(parents=True,exist_ok=True)
summary=collections.Counter();rows=[]
for ref in refs:
 name=Path(ref).name
 source=DECODE/(name+'.json')
 if not source.is_file():
  summary['missing']+=1
  rows.append({'source':ref,'status':'missing'});continue
 try:
  data=imp.export_animation(imp.load_resource(source))
  identifier=hashlib.sha256(ref.encode()).hexdigest()[:10]
  target=(Path(ref).stem+'_'+identifier+'.animations.json')
  (EXPORT/target).write_text(json.dumps(data,ensure_ascii=False,indent=2)+'\n','utf8')
  clips=len(data['clips'])
  tracks=sum(len(c['tracks']) for c in data['clips'])
  interps=sum(len(t['interpolators']) for c in data['clips'] for t in c['tracks'])
  events=sum(len(t['events']) for c in data['clips'] for t in c['tracks'])
  summary.update(exported=1,clips=clips,tracks=tracks,interpolators=interps,events=events)
  rows.append({'source':ref,'asset':'ui/ink/generated/hud/animations/'+target,
               'status':'exported','clips':clips,'tracks':tracks,'interpolators':interps,'events':events})
 except Exception as e:
  summary['failed']+=1
  rows.append({'source':ref,'status':'error','error':str(e)[:280]})
result={'schema':'newviso.ink.hud_source_animations.v1','counts':dict(summary),
        'items':rows,'source_shaders_executed':False,'controllers_implemented':False}
(EXPORT/'hud_source_animations_manifest.json').write_text(
 json.dumps(result,ensure_ascii=False,indent=2)+'\n','utf8')
print(json.dumps(result['counts'],ensure_ascii=False))
print('errors',[(x['source'],x.get('error')) for x in rows if x['status']=='error'][:7])
