#!/usr/bin/env python3
"""Convert HUD-only original CR2W widgets; preserve source depot identity."""
import json,shutil,subprocess,time
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
PROJ=ROOT/'Projects/KaylaNightCity'
SOURCE=PROJ/'.newviso/cache/ink_full_ui_original'
STAGE=ROOT/'NewViso/cache/hud_source_transfer/staged'
DEST=PROJ/'.newviso/cache/ink_full_ui_decoded/hud_widgets'
CLI=ROOT/'Tools/third_party/WolvenKitCLI'
data=json.loads((PROJ/'ui/ink/original_hud_registry.json').read_text('utf8'))
refs=sorted({e['widget'].replace(chr(92),'/') for d in data['documents'] for e in d['entries'] if e.get('widget')})
missing=[];staged=[]
for r in refs:
 p=Path(r)
 if p.is_absolute() or '..' in p.parts or not r.endswith('.inkwidget') or not (SOURCE/p).exists():
  missing.append(r);continue
 t=STAGE/p;t.parent.mkdir(parents=True,exist_ok=True)
 shutil.copy2(SOURCE/p,t);staged.append(r)
DEST.mkdir(parents=True,exist_ok=True)
args=[str(CLI/'dotnet10/dotnet.exe'),str(CLI/'10.0.0-beta.1/WolvenKit.CLI.dll'),
      'convert','serialize',str(STAGE),'-w','*.inkwidget','-o',str(DEST)]
t=time.time()
run=subprocess.run(args,capture_output=True,text=True,errors='replace',timeout=245)
report={'requested':len(refs),'staged':len(staged),'missing':missing,
 'result':run.returncode,'json_count':len(list(DEST.rglob('*.inkwidget.json'))),
 'duration_sec':round(time.time()-t,1),'log_tail':run.stdout[-1800:],'errors':run.stderr[-400:]}
out=ROOT/'NewViso/cache/hud_source_transfer/hud_decode_report.json'
out.write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n',encoding='utf8')
print(json.dumps({k:v for k,v in report.items() if k!='missing'},ensure_ascii=False))
