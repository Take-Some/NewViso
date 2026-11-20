#!/usr/bin/env python3
"""Generate a TypeScript sampler from the ORIGINAL HUD-phone Ink intro curve."""
import json
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
P=ROOT/'Projects/KaylaNightCity'
H=P/'ui/ink/generated/hud/animations'
index=json.loads((H/'hud_source_animations_manifest.json').read_text('utf8'))
matches=[r for r in index['items'] if r.get('source','').endswith('/hud_phone_animations.inkanim')]
if len(matches)!=1:raise ValueError('source phone animation must resolve uniquely')
source=json.loads((P/matches[0]['asset']).read_text('utf8'))
clips=[c for c in source['clips'] if c.get('name')=='widgetIntro']
if len(clips)!=1:raise ValueError('original widgetIntro is missing')
tracks=[t for t in clips[0]['tracks'] if t.get('target_path')==[0,0,0]]
if len(tracks)!=1:raise ValueError('original widget root target is ambiguous')
segments=[]
for v in tracks[0]['interpolators']:
 if v.get('$type')!='inkanimTransparencyInterpolator':continue
 segments.append({'start':v['startDelay'],'duration':v['duration'],
    'a':v['startValue'],'b':v['endValue'],
    'type':v.get('interpolationType','Linear'),
    'mode':v.get('interpolationMode','EasyIn')})
if len(segments)!=9:raise ValueError('unexpected phone Ink intro source curve')
segments.sort(key=lambda x:x['start'])
duration=max(s['start']+s['duration'] for s in segments)
target=P/'scripts/generated/ink_phone_intro_source.ysc'
target.parent.mkdir(parents=True,exist_ok=True)
source_text='''// Generated from original Cyberpunk 2077 HUD phone widgetIntro root track.
 // Source values are unchanged. UI layout/renderer remains NewViso-owned.
 export const INK_PHONE_INTRO_DURATION=__DURATION__;
 const SEGMENTS=__SEGMENTS__ as const;
 export function sampleOriginalPhoneIntro(t:number):number{
  const elapsed=Math.max(0,Number.isFinite(t)?t:0);
  let item=SEGMENTS[0];
  for(const candidate of SEGMENTS)if(candidate.start<=elapsed)item=candidate;
  const duration=Math.max(0,item.duration);
  const fraction=duration<=1e-9?(elapsed>=item.start?1:0):
    Math.max(0,Math.min(1,(elapsed-item.start)/duration));
  const exponent=item.type==="Quadratic"?2:item.type==="Cubic"?3:
    item.type==="Quartic"?4:item.type==="Quintic"?5:1;
  let curve=fraction;
  if(item.mode==="EasyOut")curve=1-Math.pow(1-fraction,exponent);
  else if(item.mode==="EasyInOut"){
   curve=fraction<.5?.5*Math.pow(fraction*2,exponent):
    1-.5*Math.pow((1-fraction)*2,exponent);
  }else curve=Math.pow(fraction,exponent);
  return Math.max(0,Math.min(1,item.a+(item.b-item.a)*curve));
 }
'''.replace('__DURATION__',str(duration)).replace('__SEGMENTS__',json.dumps(segments,ensure_ascii=False))
target.write_text(source_text,'utf8')
print('OUTPUT',target,'duration',duration,'keys',len(segments),
      'at zero=',segments[0]['a'],'last=',segments[-1]['b'])
