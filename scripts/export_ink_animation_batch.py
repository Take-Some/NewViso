#!/usr/bin/env python3
"""Export real locally decoded REDengine Ink animation curves and event metadata."""
from __future__ import annotations
import argparse
from collections import Counter
import hashlib
import importlib.util
import json
from pathlib import Path

HERE=Path(__file__).resolve().parent
PROJECT=HERE.parents[1]/'Projects'/'KaylaNightCity'

def importer_module():
    spec=importlib.util.spec_from_file_location('ink_source_importer', HERE/'import_redengine_ink.py')
    obj=importlib.util.module_from_spec(spec)
    spec.loader.exec_module(obj)
    return obj

def run(project, limit=0):
    source=project/'.newviso/cache/ink_full_ui_decoded'
    output=project/'ui/ink/generated/animations'
    output.mkdir(parents=True,exist_ok=True)
    importer=importer_module()
    library=[]
    totals=Counter()
    event_types=Counter()
    interpolator_types=Counter()
    input_files=sorted(source.rglob('*.inkanim.json'))
    if limit:input_files=input_files[:limit]
    for f in input_files:
        identity=f.relative_to(source).as_posix()
        try:
            data=importer.load_resource(f)
            exported=importer.export_animation(data)
            depot=data.get('Header',{}).get('ArchiveFileName','')
            digest=hashlib.sha256(identity.encode('utf-8')).hexdigest()[:12]
            dest=output/(f.stem.replace('.inkanim','')+'_'+digest+'.json')
            dest.write_text(json.dumps(exported,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
            tracks=sum(len(c.get('tracks',[])) for c in exported['clips'])
            interps=sum(len(t.get('interpolators',[])) for c in exported['clips'] for t in c.get('tracks',[]))
            events=sum(len(t.get('events',[])) for c in exported['clips'] for t in c.get('tracks',[]))
            for clip in exported['clips']:
                for track in clip.get('tracks',[]):
                    interpolator_types.update(item.get('$type','unknown')
                        for item in track.get('interpolators',[]))
                    event_types.update(item.get('Data',item).get('$type','unknown')
                        for item in track.get('events',[]))
            totals.update(clips=len(exported['clips']),tracks=tracks,interpolators=interps,events=events,converted=1)
            library.append({'source':identity,'depot':depot,'asset':'ui/ink/generated/animations/'+dest.name,
                            'clips':len(exported['clips']),'tracks':tracks,'interpolators':interps,
                            'events':events,'status':'source_curves_not_bound'})
        except Exception as exc:
            totals['failed']+=1
            library.append({'source':identity,'status':'import_error','error':str(exc)[:300]})
    manifest={'schema':'newviso.ink.animation_batch.v1','libraries':library,'counts':dict(totals),
              'interpolator_types':dict(interpolator_types),
              'event_types':dict(event_types),
              'original_effect_shaders_executed':False,'original_controllers_executed':False,
              'runtime_binding_verified':False}
    (output/'source_animation_manifest.json').write_text(
        json.dumps(manifest,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    return {'input':len(input_files),'counts':dict(totals)}

if __name__=='__main__':
    parser=argparse.ArgumentParser()
    parser.add_argument('--project',type=Path,default=PROJECT)
    parser.add_argument('--limit',type=int,default=0)
    args=parser.parse_args()
    print(json.dumps(run(args.project.resolve(),args.limit),ensure_ascii=False))
