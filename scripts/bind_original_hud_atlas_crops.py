#!/usr/bin/env python3
"""Bind exact original atlas sprite PNGs to compiled HUD Ink preview widgets."""
import json,re,collections
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
P=ROOT/'Projects/KaylaNightCity'
SURF=P/'ui/ink/generated/hud'
PNG=P/'assets/ui/ink'
ATLAS={
 'base/gameplay/gui/common/icons/atlas_common.inkatlas':'atlas_common',
 'base/gameplay/gui/common/shapes/atlas_shapes_sync.inkatlas':'atlas_shapes_sync',
 'base/gameplay/gui/fullscreen/main_menu/menu_background.inkatlas':'menu_background',
 'base/gameplay/gui/fullscreen/hub_menu/new_hub_layout.inkatlas':'new_hub_layout'
}
index={}
for depot,folder in ATLAS.items():
 parts=collections.defaultdict(list)
 for image in sorted((PNG/folder).glob('*.png')):
  match=re.fullmatch(r'slot(\d+)_\d+_(.+)',image.stem)
  if match:
   parts[match.group(2)].append((int(match.group(1)),
      'assets/ui/ink/'+folder+'/'+image.name))
 index[depot]=parts
counts=collections.Counter()
for path in SURF.glob('*.surface.json'):
 data=json.loads(path.read_text('utf8'))
 changed=0
 for node in data.get('components',[]):
  props=node.get('props',{})
  atlas=props.get('ink_atlas')
  if not atlas:continue
  part=props.get('ink_part')
  if not isinstance(atlas,str) or not isinstance(part,str):
   counts['invalid_atlas_or_part']+=1;continue
  atlas=atlas.replace(chr(92),'/').lower()
  candidates=index.get(atlas,{}).get(part,[])
  if not candidates:
   counts['not_extracted']+=1;continue
  # The original atlases may contain multiple resolution slots. Use only
  # source-proven slot 0; never choose a random image.
  slots=[ref for slot,ref in candidates if slot==0]
  if len(slots)!=1:
   counts['ambiguous_slot']+=1;continue
  ref=slots[0]
  if not (P/ref).is_file():
   counts['missing_file']+=1;continue
  props['texture_ref']=ref
  props['texture_mode']='stretch'
  props['tint_rgba']=props.get('ink_tint',[255,255,255,255])
  props['ink_source_atlas_raster_bound']=True
  if props.get('ink_nine_slice'):props['ink_nine_slice_unimplemented']=True
  counts['bound']+=1;changed+=1
 if changed:
  path.write_text(json.dumps(data,ensure_ascii=False,indent=2)+'\n','utf8')
  counts['surfaces_changed']+=1
report={'schema':'newviso.ink.hud_atlas_binding.v1','counts':dict(counts),
        'original_atlas_sources':ATLAS,'unresolved_is_not_a_substitute':True}
(SURF/'hud_atlas_binding_manifest.json').write_text(
 json.dumps(report,ensure_ascii=False,indent=2)+'\n','utf8')
print(json.dumps(report['counts'],ensure_ascii=False))
