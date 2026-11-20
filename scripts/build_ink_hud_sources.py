#!/usr/bin/env python3
"""Source-faithful Ink HUD registry transfer, with explicit parity flags."""
import json
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
PROJECT=ROOT/'Projects'/'KaylaNightCity'
def main():
    path=PROJECT/'ui/ink/original_hud_registry.json'
    registry=json.loads(path.read_text('utf-8-sig'))
    if registry.get('schema')!='newviso.ink.hud_registry.v1':
        raise ValueError('Unknown source HUD schema')
    docs=registry['documents']
    target=ROOT/'NewViso/cache/hud_source_transfer'
    target.mkdir(parents=True,exist_ok=True)
    widgets=sorted({e['widget'] for d in docs for e in d['entries'] if e.get('widget')})
    entries=sum(len(d['entries']) for d in docs)
    payload={'schema':'newviso.ink.hud.source_snapshot.v1','profiles':docs,'widget_paths':widgets,'source_entries':entries}
    (target/'source_registry_snapshot.json').write_text(
        json.dumps(payload,ensure_ascii=False,indent=2)+chr(10),encoding='utf-8')
    print(json.dumps({'profiles':len(docs),'entries':entries,
        'unique_widgets':len(widgets)},ensure_ascii=False))

if __name__=='__main__':main()
