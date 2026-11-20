#!/usr/bin/env python3
"""Bind decoded Ink atlas names/parts to actual project-owned source PNG crops.
The source path and part name must match exactly. No procedural visual fallback.
"""
import argparse
import json
from pathlib import Path


def main():
    ap=argparse.ArgumentParser()
    ap.add_argument("--surface-dir",type=Path,required=True)
    ap.add_argument("--asset-root",type=Path,required=True)
    ap.add_argument("--atlas-name",required=True,
                    help="Original REDengine .inkatlas depot path")
    ap.add_argument("--manifest",type=Path,required=True)
    ap.add_argument("--resource-prefix",required=True,
                    help="VFS-relative folder containing the source PNG parts")
    args=ap.parse_args()
    manifest=json.loads(args.manifest.read_text(encoding="utf-8-sig"))
    by_name={part["name"]:part for part in manifest}
    path=args.atlas_name.lower().replace("\\","/")
    matched=0
    unresolved=0
    changed=0
    for surface_path in sorted(args.surface_dir.glob("*.surface.json")):
        surface=json.loads(surface_path.read_text(encoding="utf8"))
        for node in surface.get("components",[]):
            props=node.get("props",{})
            if str(props.get("ink_atlas") or "").lower().replace("\\","/")!=path:
                continue
            part=by_name.get(props.get("ink_part"))
            if not part:
                unresolved+=1
                continue
            ref=(args.resource_prefix.rstrip("/")+"/"+part["filename"]).replace("\\","/")
            target=args.asset_root/ref
            if not target.is_file():
                unresolved+=1
                continue
            props["texture_ref"]=ref
            props["texture_mode"]="tile" if props.get("ink_tiling") in ("Horizontal","Vertical","Both") else "stretch"
            props["tint_rgba"]=props.get("ink_tint",[255,255,255,255])
            if props.get("ink_nine_slice"):
                props["ink_nine_slice_unimplemented"]=True
            matched+=1
        surface["metrics"]["source_atlas_binding"]="source-cr2w-uv-crop"
        surface_path.write_text(json.dumps(surface,ensure_ascii=False,indent=2)+"\n",encoding="utf8")
        changed+=1
    print(json.dumps({"surfaces":changed,"bound_images":matched,
                      "unresolved_matching_atlas_parts":unresolved},
                     ensure_ascii=False))


if __name__=="__main__":
    main()
