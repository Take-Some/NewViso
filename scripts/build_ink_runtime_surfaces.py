#!/usr/bin/env python3
"""Compile *layout and visible primitives* from original Ink CR2W JSON into
provider-neutral NewViso UiSurfaceNode JSON.

The conversion does NOT implement game controller code, material shaders,
Ink video, virtual list data sources, localization or animation callbacks.
Those remain explicit diagnostics and are never silently emulated.

Usage:
    python build_ink_runtime_surfaces.py --source <dir> --output <dir> --screens ...
"""
from __future__ import annotations
import argparse
import importlib.util
import json
import math
from pathlib import Path
from collections import Counter
import sys

MAX_NODES = 8000
DESIGN_SIZE = (1600, 900)
SOURCE_SIZE = (3840, 2160)
WIDGET_MAP = {
    "inkCanvasWidget": "panel",
    "inkFlexWidget": "panel",
    "inkVerticalPanelWidget": "column",
    "inkHorizontalPanelWidget": "row",
    "inkCompoundWidget": "panel",
    "inkBorderWidget": "panel",
    "inkRectangleWidget": "panel",
    "inkCircleWidget": "panel",
    "inkMaskWidget": "panel",
    "inkShapeWidget": "panel",
    "inkTextWidget": "text",
    "inkRichTextWidget": "text",
    "inkImageWidget": "external_texture",
    "inkScrollAreaWidget": "column",
    "inkVideoWidget": "panel",
    "inkGridWidget": "grid",
}


def refvalue(value):
    if isinstance(value, dict):
        if "$value" in value: return value["$value"]
        if "DepotPath" in value: return refvalue(value["DepotPath"])
    return value


def n(value, default=0.0):
    try:
        x = float(value)
        return x if math.isfinite(x) else default
    except (TypeError, ValueError):
        return default


def vec(value, x=0.0, y=0.0):
    if not isinstance(value, dict): return x, y
    return n(value.get("X"),x),n(value.get("Y"),y)


def color(tint, opacity):
    tint = tint or {}
    alpha = max(0.0,min(1.0,n(opacity,1.0)*n(tint.get("Alpha"),1.0)))
    return [round(max(0,min(1,n(tint.get(k),1.0)))*255)
            for k in ("Red","Green","Blue")] + [round(alpha*255)]


def props_of(widget):
    return widget.get("props", {})


def layout_rect(src, parent, flex_offset=(0.0,0.0)):
    """Evaluate source fixed size/anchor/margin/transform in source pixels.
    Unknown dynamic/flex sizes are tracked, not claimed to be exact.
    """
    p = props_of(src)
    lay = p.get("ink_layout") or {}
    margin = lay.get("margin") or {}
    transform = (p.get("ink_renderTransform") or {})
    sw,sh = vec(p.get("ink_size"))
    scale_x,scale_y = vec(transform.get("scale"),1,1)
    w = abs(sw*scale_x)
    h = abs(sh*scale_y)
    px,py,pw,ph = parent
    sizing_approx = w<0.1 or h<0.1
    if w<0.1:
        w = min(pw,max(60., n(lay.get("sizeCoefficient"),1)*pw))
    if h<0.1:
        h = min(ph,max(30., n(lay.get("sizeCoefficient"),1)*ph))
    anchor = str(lay.get("anchor") or "TopLeft").lower()
    ux,uy = vec(lay.get("anchorPoint"))
    if anchor in ("centered","center"):
        ux,uy=.5,.5
    elif anchor=="topcenter":ux,uy=.5,0.
    elif anchor=="bottomcenter":ux,uy=.5,1.
    elif anchor=="centerleft":ux,uy=0.,.5
    elif anchor=="centerright":ux,uy=1.,.5
    elif anchor=="bottomleft":ux,uy=0.,1.
    elif anchor=="bottomright":ux,uy=1.,1.
    elif anchor=="topright":ux,uy=1.,0.
    elif anchor=="topleft":ux,uy=0.,0.
    elif "fill" in anchor:
        if "vertical" in anchor:ux,uy=0.,.5
    mt=n(margin.get("top"));ml=n(margin.get("left"))
    mb=n(margin.get("bottom"));mr=n(margin.get("right"))
    tx,ty=vec(transform.get("translation"))
    ax=px+ux*pw
    ay=py+uy*ph
    if ux==0:x=ax+ml
    elif ux==1:x=ax-w-mr
    else:x=ax-w*.5+ml-mr
    if uy==0:y=ay+mt
    elif uy==1:y=ay-h-mb
    else:y=ay-h*.5+mt-mb
    if flex_offset!=(0.,0.):
        x+=flex_offset[0];y+=flex_offset[1]
    return (x+tx,y+ty,w,h),sizing_approx


def relative_source_nodes(root, font_map=None):
    font_map=font_map or {}
    result=[]
    gaps=Counter()
    status=Counter()
    def visit(node,parent,offset=(0.,0.),depth=0,path=()):
        if depth>70:raise ValueError("Ink tree too deep")
        if len(result)>MAX_NODES:raise ValueError("Ink surface too large")
        p=props_of(node)
        kind=p.get("ink_class","")
        name=p.get("ink_name","")
        if p.get("ink_visible") in (False,0):
            status['hidden']+=1
            return
        rect,approx=layout_rect(node,parent,offset)
        if approx:status['approximate_dimensions']+=1
        status['widgets']+=1
        # Source Ink controller assigns the actual dimensions of some
        # initially-zero leaf widgets. Never expand them to a parent-sized
        # raster image: this produces fullscreen visual corruption.
        if approx and kind in (
            "inkImageWidget", "inkTextWidget", "inkRichTextWidget",
            "inkRichTextBoxWidget", "inkBorderWidget", "inkRectangleWidget",
            "inkCircleWidget", "inkShapeWidget", "inkMaskWidget",
        ):
            status["dynamic_leaf_geometry_deferred"] += 1
            gaps["runtime_controller_sized_widget"] += 1
            return
        component=WIDGET_MAP.get(kind)
        if not component:
            gaps[kind]+=1;component="panel"
        # Flatten absolute positions so all provider implementations use the
        # same source-space results, not provider-specific flex placement.
        x,y,w,h=rect
        sx,sy=DESIGN_SIZE[0]/SOURCE_SIZE[0],DESIGN_SIZE[1]/SOURCE_SIZE[1]
        node_props={
            "position":"absolute",
            "x_px":round(max(0,x*sx),3),
            "y_px":round(max(0,y*sy),3),
            "w_px":round(max(1,w*sx),3),
            "h_px":round(max(1,h*sy),3),
            "ink_source_path":p.get("ink_name",""),
            "ink_animation_target_path":"/".join(map(str,path)),
            "ink_source_class":kind,
            "ink_source_handle":p.get("ink_handle_id"),
            "ink_source_rect":[round(v,3) for v in rect],
            "ink_layout_approximate":approx,
        }
        if kind in ("inkCanvasWidget","inkCompoundWidget",
                    "inkFlexWidget","inkHorizontalPanelWidget",
                    "inkVerticalPanelWidget","inkGridWidget"):
            node_props['draw_panel']=False
            # Pure containers are discarded and their children are flattened.
        elif kind in ("inkBorderWidget","inkRectangleWidget",
                      "inkCircleWidget","inkShapeWidget","inkMaskWidget"):
            c=color(p.get("ink_tintColor"),p.get("ink_opacity"))
            node_props.update(draw_panel=True,fill_rgba=c, border_rgba=c,
                              border_px=1 if kind=="inkBorderWidget" else 0)
        elif kind in ("inkTextWidget","inkRichTextWidget"):
            source_font=str(p.get("ink_fontFamily") or "").replace("\\","/").lower()
            style=str(refvalue(p.get("ink_fontStyle")) or "Regular")
            family=font_map.get(source_font)
            node_props.update(
                font_px=round(max(8,n(p.get("ink_fontSize"),32)*sx),2),
                text_rgba=color(p.get("ink_tintColor"),p.get("ink_opacity")),
                font_style=style,
                ink_font_source=source_font,
                ink_font_unresolved=family is None)
            if family:
                node_props["font_ref"]=f"font://cyberpunk/{family}/{style}"
        elif kind=="inkImageWidget":
            atlas=p.get("ink_textureAtlas")
            part=refvalue(p.get("ink_texturePart"))
            node_props.update({
                "ink_atlas":atlas,
                "ink_part":part,
                "ink_tint":color(p.get("ink_tintColor"),p.get("ink_opacity")),
                "ink_nine_slice":bool(p.get("ink_useNineSliceScale")),
                "ink_tiling":p.get("ink_tileType"),
                "draw_panel":False})
            # Missing atlas pixels are NOT substituted with unrelated project
            # assets. Resource binding is a separate deterministic stage.
            status['unresolved_image_pixels']+=1
        elif kind=="inkVideoWidget":
            node_props.update(
                draw_panel=False,
                ink_video_unimplemented=True)
            status['video_unimplemented']+=1
        elif kind=="inkScrollAreaWidget":
            node_props.update(draw_panel=False,scroll_y=True)
        inter=bool(p.get("ink_isInteractive"))
        if inter:
            node_props["interactive"]=True
            status["interactive_without_controller"]+=1
        translated={
            "id":str(node.get("id","ink.node")),
            "component_id":component,
            "props":node_props
        }
        text=node.get("text")
        if kind in ("inkTextWidget","inkRichTextWidget") and isinstance(text,str):
            translated["text"]=text
            if text.startswith("LocKey#"):
                translated["props"]["ink_localization_unresolved"]=True
                status['unresolved_loc_keys']+=1
        # Only render primitives with source semantics. Do not flood
        # interactive UI with invisible buttons or empty panels.
        if kind not in ("inkCanvasWidget","inkCompoundWidget","inkFlexWidget",
                        "inkHorizontalPanelWidget","inkVerticalPanelWidget",
                        "inkGridWidget","inkVideoWidget"):
            result.append(translated)
        if kind=="inkVideoWidget":
            gaps["inkVideoWidget:video_pixels"]+=1
        children=node.get('children',[])
        is_column=kind in ("inkVerticalPanelWidget",)
        is_row=kind in ("inkHorizontalPanelWidget",)
        current=0.
        for child_idx,child in enumerate(children):
            extra=(0., current) if is_column else ((current,0.) if is_row else (0.,0.))
            visit(child,rect,extra,depth+1,path+(child_idx,))
            if is_column or is_row:
                sz=vec(props_of(child).get("ink_size"))
                current += max(0,sz[1] if is_column else sz[0])
    visit(root,(0.,0.,float(SOURCE_SIZE[0]),float(SOURCE_SIZE[1])))
    return result,dict(status),dict(gaps)


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source",required=True,type=Path,
                        help="Directory of WolvenKit-serialized CR2W JSON")
    parser.add_argument("--importer",required=True,type=Path,
                        help="NewViso/scripts/import_redengine_ink.py")
    parser.add_argument("--output",required=True,type=Path)
    parser.add_argument("--animations-source-dir",type=Path,
                        help="Optional locally serialized original .inkanim CR2W files")
    parser.add_argument("--font-catalog",type=Path,
                        help="Extracted REDengine inkfontfamily mapping catalog")
    parser.add_argument("--screens",nargs="+",default=[
        "singleplayer_menu","new_game_menu","difficulty_level",
        "load_game_menu","save_game_menu","settings_main","pause_menu"])
    args=parser.parse_args()
    spec=importlib.util.spec_from_file_location("newviso_ink_raw_import",args.importer)
    module=importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    summaries=[]
    font_map={}
    if args.font_catalog:
        catalog=json.loads(args.font_catalog.read_text(encoding="utf8"))
        for family in catalog.get("families",[]):
            path=family.get("path","").replace("\\","/").lower()
            if path:font_map[path]=family["family"]
    args.output.mkdir(parents=True,exist_ok=True)
    for screen in args.screens:
        matches=list(args.source.glob(screen+".inkwidget.json"))
        if len(matches)!=1:
            print(f"SKIP {screen}: source match count {len(matches)}",file=sys.stderr)
            continue
        original=module.load_resource(matches[0])
        importer=module.InkImporter(original)
        tree,report=importer.convert_surface("Root",f"newviso.ink.source.{screen}")
        nodes,stats,gaps=relative_source_nodes(tree['components'][0],font_map)
        surface={
            "version":1,"surface_id":f"newviso.ink.source.{screen}",
            "source":"cyberpunk-ink-cr2w","visible":False,"modal":False,
            "z_order":6000,"title":"","subtitle":"",
            "body_lines":[],"footer_lines":[],
            "style_tags":["fullscreen","responsive-menu","surface-chrome:none",
                          "source-ink","no-text-selection"],
            "metrics":{"ink_depot_resource":matches[0].name,
                "ink_source_resolution":list(SOURCE_SIZE),
                "ink_source_item":"Root",
                "ink_runtime_compatible":True,
                "ink_visual_parity_verified":False,
                "ink_controller_parity_verified":False,
                "ink_unimplemented":gaps},
            "style":{
                "theme_id":"cyberpunk-source-ink",
                "panel_rgba":[0,0,0,0],
                "backdrop_rgba":[0,0,0,0],
                "text_rgba":[235,239,243,255],
                "accent_rgba":[252,231,76,255],
                "border_rgba":[0,0,0,0],
                "padding_px":[0,0,0,0],
                "margin_px":[0,0],
                "min_size_px":list(DESIGN_SIZE),
                "max_size_px":list(DESIGN_SIZE),
            },
            "components":nodes
        }
        animation_path=(args.animations_source_dir/(screen+"_animations.inkanim.json")
            if args.animations_source_dir else None)
        if animation_path is not None and animation_path.is_file():
            library=module.export_animation(module.load_resource(animation_path))
            node_paths={}
            for node in nodes:
                key=node.get("props",{}).get("ink_animation_target_path")
                if key and key not in node_paths:
                    node_paths[key]=node["id"]
            clip_routes={}
            bound=unbound=0
            for original_clip in library.get("clips",[]):
                routes={}
                for track in original_clip.get("tracks",[]):
                    indices=track.get("target_path") or []
                    key="/".join(str(part) for part in indices)
                    target=node_paths.get(key)
                    if target:
                        routes[key]=target
                        bound+=1
                    else:unbound+=1
                clip_routes[original_clip["name"]]=routes
            anim_target=args.output/(screen+".animations.json")
            anim_target.write_text(json.dumps(library,ensure_ascii=False,indent=2)+"\n",
                encoding="utf8")
            surface["metrics"]["ink_animation"]={
                "asset":"ui/ink/generated/"+anim_target.name,
                "clips":{"intro":"intro","loop":"loop",
                    "hover_enter":"pause_button_hover_over_anim",
                    "hover_leave":"pause_button_hover_out_anim"},
                "bindings":clip_routes,
            }
            surface["metrics"]["ink_animation_binding_stats"]={
                "bound_tracks":bound,"unbound_tracks":unbound,
                "clips":len(library["clips"])
            }
        path=args.output/(screen+".surface.json")
        path.write_text(json.dumps(surface,ensure_ascii=False,indent=2)+"\n",
                        encoding="utf8")
        summaries.append({"screen":screen,"nodes":len(nodes),"status":stats,
            "unimplemented":gaps,"original_widgets":report["widget_count"]})
    # Incremental compilation must retain status for all previously built
    # surfaces; an animation-only rebuild must not erase fullscreen reports.
    index=args.output/"build_report.json"
    existing={}
    if index.is_file():
        try:
            previous=json.loads(index.read_text(encoding="utf8"))
            if previous.get("schema")=="newviso.ink.surface_build.report.v1":
                existing={row["screen"]:row for row in previous.get("screens",[])
                    if isinstance(row,dict) and isinstance(row.get("screen"),str)}
        except (ValueError,OSError):
            existing={}
    existing.update({row["screen"]:row for row in summaries})
    report={"schema":"newviso.ink.surface_build.report.v1",
            "screens":[existing[name] for name in sorted(existing)],
            "limitations":[
                "Visual surfaces omit missing source atlas/XBM resources",
                "Controller classes are metadata only until explicitly adapted",
                "Flex layout and dynamic virtual lists need measured runtime constraints",
                "Original video, effects and localization are not implemented",
            ]}
    (args.output/"build_report.json").write_text(
        json.dumps(report,ensure_ascii=False,indent=2)+"\n",encoding="utf8")
    print(json.dumps({"built":len(summaries),"screens":summaries},
                     ensure_ascii=False))


if __name__=="__main__":
    main()
