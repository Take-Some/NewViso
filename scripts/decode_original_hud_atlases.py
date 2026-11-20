#!/usr/bin/env python3
"""Decode exact atlas metadata referenced by original HUD, inventory XBM sources."""
import importlib.util,json,shutil,subprocess,collections,hashlib
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
P=ROOT/'Projects/KaylaNightCity'
ORIG=P/'.newviso/cache/ink_full_ui_original'
STAGE=ROOT/'NewViso/cache/hud_source_transfer/staged_atlases'
DEST=P/'.newviso/cache/ink_full_ui_decoded/hud_atlases'
OUT=P/'ui/ink/generated/hud/atlas_sources'
CLI=ROOT/'Tools/third_party/WolvenKitCLI'
refs=sorted({n['props']['ink_atlas'] for f in (P/'ui/ink/generated/hud').glob('*.surface.json')
 for n in json.loads(f.read_text())['components'] if isinstance(n['props'].get('ink_atlas'),str)})
missing=[]
for ref in refs:
 relative=Path(ref.replace(chr(92),'/'))
 if relative.is_absolute() or '..' in relative.parts or not (ORIG/relative).is_file():
  missing.append(ref);continue
 p=STAGE/relative;p.parent.mkdir(parents=True,exist_ok=True)
 shutil.copy2(ORIG/relative,p)
DEST.mkdir(parents=True,exist_ok=True)
OUT.mkdir(parents=True,exist_ok=True)
(OUT/'source_items').mkdir(parents=True,exist_ok=True)
args=[str(CLI/'dotnet10/dotnet.exe'),str(CLI/'10.0.0-beta.1/WolvenKit.CLI.dll'),
 'convert','serialize',str(STAGE),'-w','*.inkatlas','-o',str(DEST)]
cp=subprocess.run(args,capture_output=True,text=True,errors='replace',timeout=230)
spec=importlib.util.spec_from_file_location('ink_import',ROOT/'NewViso/scripts/import_redengine_ink.py')
imp=importlib.util.module_from_spec(spec);spec.loader.exec_module(imp)
rows=[];xbms=collections.Counter();summary=collections.Counter()
for ref in refs:
 path=DEST/(Path(ref).name+'.json')
 if not path.exists():continue
 try:
  data=imp.export_atlas(imp.load_resource(path))
  for v in data['textures']:
   if v:xbms[v]+=1
  name=Path(ref).stem
  (OUT/'source_items'/(name+'_'+hashlib.sha256(ref.encode()).hexdigest()[:10]+'.atlas.json')).write_text(json.dumps(data,ensure_ascii=False,indent=2)+'\n','utf8')
  rows.append({'source':ref,'texture_refs':data['textures'],'parts':len(data['parts']),
   'asset':'ui/ink/generated/hud/atlas_sources/source_items/'+name+'_'+hashlib.sha256(ref.encode()).hexdigest()[:10]+'.atlas.json'})
  summary.update(converted=1,parts=len(data['parts']))
 except Exception as exc:
  summary['failed']+=1;rows.append({'source':ref,'error':str(exc)[:240]})
record={'schema':'newviso.ink.hud_atlas_sources.v1','requested':len(refs),
 'missing_original_atlases':missing,'cli_code':cp.returncode,'summary':dict(summary),
 'unique_xbm_references':len(xbms),'xbm_refs':dict(xbms),'atlases':rows,
 'pixel_sources_resolved':False}
(OUT/'hud_source_atlases_manifest.json').write_text(json.dumps(record,ensure_ascii=False,indent=2)+'\n','utf8')
print(json.dumps({k:v for k,v in record.items() if k not in ('xbm_refs','atlases','missing_original_atlases')},ensure_ascii=False))
print('missing atlas count',len(missing))
