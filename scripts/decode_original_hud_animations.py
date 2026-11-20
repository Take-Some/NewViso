#!/usr/bin/env python3
"""Decode exact HUD Ink animation references without editing source files."""
import json,shutil,subprocess,time
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
PROJECT=ROOT/'Projects/KaylaNightCity'
ORIG=PROJECT/'.newviso/cache/ink_full_ui_original'
STAGE=ROOT/'NewViso/cache/hud_source_transfer/staged_animations'
DEST=PROJECT/'.newviso/cache/ink_full_ui_decoded/hud_animations'
CLI=ROOT/'Tools/third_party/WolvenKitCLI'
index=json.loads((PROJECT/'ui/ink/generated/hud/source_hud_surfaces_manifest.json').read_text('utf8'))
refs=sorted({r['source_animation'] for r in index['widgets'] if r.get('source_animation')})
missing=[];staged=[]
for r in refs:
 p=Path(r.replace(chr(92),'/'))
 if p.is_absolute() or '..' in p.parts or not (ORIG/p).is_file():
  missing.append(r);continue
 target=STAGE/p;target.parent.mkdir(parents=True,exist_ok=True)
 shutil.copy2(ORIG/p,target);staged.append(r)
DEST.mkdir(parents=True,exist_ok=True)
command=[str(CLI/'dotnet10/dotnet.exe'),str(CLI/'10.0.0-beta.1/WolvenKit.CLI.dll'),
 'convert','serialize',str(STAGE),'-w','*.inkanim','-o',str(DEST)]
start=time.time()
result=subprocess.run(command,capture_output=True,text=True,errors='replace',timeout=235)
report={'references':len(refs),'staged':len(staged),'missing':missing,
 'exit_code':result.returncode,'decoded':len(list(DEST.rglob('*.inkanim.json'))),
 'seconds':round(time.time()-start,2),'stdout_tail':result.stdout[-1300:]}
(DEST/'hud_decode_report.json').write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n','utf8')
print(json.dumps({k:v for k,v in report.items() if k!='missing'},ensure_ascii=False))
