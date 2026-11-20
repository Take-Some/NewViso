#!/usr/bin/env python3
"""Compile decoded original inkhud widget libraries into renderable UI preview surfaces."""
from pathlib import Path
import importlib.util,json,collections
HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[1]
PROJECT=ROOT/'Projects/KaylaNightCity'
SOURCE=PROJECT/'.newviso/cache/ink_full_ui_decoded/hud_widgets'
DEST=PROJECT/'ui/ink/generated/hud'
def module(filename,name):
 spec=importlib.util.spec_from_file_location(name,HERE/filename)
 obj=importlib.util.module_from_spec(spec)
 spec.loader.exec_module(obj)
 return obj
imp=module('import_redengine_ink.py','hud_cr2w_imp')
compiler=module('build_ink_runtime_surfaces.py','hud_ui_comp')
def main():
 DEST.mkdir(parents=True,exist_ok=True)
 stats=collections.Counter();report=[]
 for path in sorted(SOURCE.glob('*.inkwidget.json')):
  screen=path.name.removesuffix('.inkwidget.json')
  try:
   original=imp.load_resource(path)
   items=original['Data']['RootChunk'].get('libraryItems',[])
   names=[imp.scalar(x.get('name')) for x in items]
   name='Root' if 'Root' in names else next((n for n in names if n),None)
   if not name:raise ValueError('no root library item')
   tree,info=imp.InkImporter(original).convert_surface(name,'newviso.ink.hud.'+screen)
   nodes,counts,gaps=compiler.relative_source_nodes(tree['components'][0])
   result={'version':1,'surface_id':'newviso.ink.hud.'+screen,
    'source':'original-ink-hud-widget','visible':False,'modal':False,
    'z_order':7500,'title':'','subtitle':'','body_lines':[],
    'footer_lines':[],'style_tags':['hud','ink-original','surface-chrome:none'],
    'metrics':{'ink_import_status':'preview_only',
      'ink_source_asset':info['source'],
      'ink_widget_item':name,'ink_source_animation':info['source_animation_ref'],
      'ink_source_resolution':[3840,2160],
      'ink_design_resolution':[1600,900],
      'ink_layout_support':'static_approximation',
      'ink_controller_executed':False,
      'ink_source_counts':counts,'ink_unresolved_features':gaps},
    'components':nodes}
   (DEST/(screen+'.surface.json')).write_text(
     json.dumps(result,ensure_ascii=False,indent=2)+'\n','utf-8')
   stats.update(compiled=1,visible_nodes=len(nodes),original_widgets=info['widget_count'])
   report.append({'widget':screen,'status':'compiled',
      'visible_nodes':len(nodes),'source_animation':info['source_animation_ref'],
      'unresolved_handles':len(info['unresolved_handles'])})
  except Exception as exc:
   stats['errors']+=1
   report.append({'widget':screen,'status':'error','error':str(exc)[:500]})
 manifest={'schema':'newviso.ink.hud_widget_surfaces.v1',
           'summary':dict(stats),'widgets':report,
           'visual_parity_verified':False,'gameplay_controller_implemented':False}
 (DEST/'source_hud_surfaces_manifest.json').write_text(
   json.dumps(manifest,ensure_ascii=False,indent=2)+'\n','utf-8')
 print(json.dumps(manifest['summary'],ensure_ascii=False))
 print('error examples',[(x['widget'],x.get('error')) for x in report if x['status']=='error'][:8])
if __name__=='__main__':main()
