#!/usr/bin/env python3
"""Import WolvenKit-serialized REDengine Ink JSON into provider-neutral NewViso
inspection artifacts. No source-game assets are embedded in this program.
Conversion is deliberately not a claim of visual or behavioral equivalence.
"""
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path

MAX_SOURCE_BYTES = 96 * 1024 * 1024
MAX_WIDGETS = 20000
MAX_DEPTH = 80
WIDGET_TYPES = {
    "inkCanvasWidget": "panel", "inkFlexWidget": "column",
    "inkHorizontalPanelWidget": "row", "inkVerticalPanelWidget": "column",
    "inkCompoundWidget": "panel", "inkBorderWidget": "panel",
    "inkTextWidget": "text", "inkImageWidget": "external_texture",
    "inkRectangleWidget": "panel", "inkCircleWidget": "panel",
    "inkMaskWidget": "panel", "inkShapeWidget": "panel",
}
WIDGET_PROPS = ("layout", "size", "renderTransform", "renderTransformPivot",
                "opacity", "tintColor", "visible", "state", "style", "effects",
                "textureAtlas", "texturePart", "useNineSliceScale", "tileType",
                "fontFamily", "fontStyle", "fontSize", "horizontalAlignment",
                "verticalAlignment", "wrappingInfo", "letterCase", "isInteractive")


def load_resource(path):
    if path.stat().st_size > MAX_SOURCE_BYTES:
        raise ValueError("Ink JSON too large: " + str(path))
    obj = json.loads(path.read_text(encoding="utf-8-sig"))
    root = obj.get("Data", {}).get("RootChunk", {})
    if obj.get("Header", {}).get("DataType") != "CR2W" or not root.get("$type", "").startswith("ink"):
        raise ValueError("Not a WolvenKit CR2W Ink JSON: " + str(path))
    return obj


def scalar(value):
    return value.get("$value") if isinstance(value, dict) and "$value" in value else value


def depot(value):
    if not isinstance(value, dict):
        return None
    entry = scalar(value.get("DepotPath"))
    if isinstance(entry, str) and entry != "0":
        return entry.replace("\\", "/")
    return None


def index_handles(root):
    handles = {}
    stack = [root]
    budget = 600000
    while stack:
        obj = stack.pop()
        budget -= 1
        if budget < 0:
            raise ValueError("CR2W object graph exceeds safe traversal budget")
        if isinstance(obj, list):
            stack.extend(obj)
        elif isinstance(obj, dict):
            value = obj.get("Data")
            if "HandleId" in obj and isinstance(value, dict):
                handles[str(obj["HandleId"])] = value
            stack.extend(obj.values())
    return handles


def deref(ref, handles):
    if not isinstance(ref, dict):
        return None, None
    if "HandleRefId" in ref:
        handle = str(ref["HandleRefId"])
        return handles.get(handle), handle
    if "HandleId" in ref:
        handle = str(ref["HandleId"])
        value = ref.get("Data", handles.get(handle))
        return value if isinstance(value, dict) else None, handle
    return ref, None


class InkImporter:
    def __init__(self, source):
        self.source = source
        self.handles = index_handles(source["Data"])
        self.types = Counter()
        self.unsupported = Counter()
        self.missing_handles = set()
        self.active = set()
        self.count = 0

    def children(self, widget):
        data, _ = deref(widget.get("children"), self.handles)
        if not isinstance(data, dict):
            return []
        children = data.get("children", [])
        return children if isinstance(children, list) else []

    def convert_node(self, entry, depth=0):
        if depth >= MAX_DEPTH:
            raise ValueError("Ink widget tree is too deep")
        widget, hid = deref(entry, self.handles)
        if widget is None:
            if hid:
                self.missing_handles.add(hid)
            return None
        kind = widget.get("$type", "")
        if not isinstance(kind, str) or not kind.startswith("ink") or not kind.endswith("Widget"):
            return None
        if hid and hid in self.active:
            return None
        self.count += 1
        if self.count > MAX_WIDGETS:
            raise ValueError("Ink widget budget exceeded")
        self.types[kind] += 1
        if kind not in WIDGET_TYPES:
            self.unsupported[kind] += 1
        name = str(scalar(widget.get("name")) or kind)
        props = {"ink_class": kind, "ink_name": name}
        if hid:
            props["ink_handle_id"] = hid
        for key in WIDGET_PROPS:
            val = widget.get(key)
            if val is not None:
                props["ink_" + key] = depot(val) or val
        node = {
            "id": "ink." + str(hid or self.count) + "." + name,
            "component_id": WIDGET_TYPES.get(kind, "panel"),
            "props": props,
        }
        text = scalar(widget.get("text"))
        if isinstance(text, str) and text:
            node["text"] = text
        if kind == "inkTextWidget":
            node["font_token"] = depot(widget.get("fontFamily")) or ""
        if hid:
            self.active.add(hid)
        children = []
        for child in self.children(widget):
            converted = self.convert_node(child, depth + 1)
            if converted is not None:
                children.append(converted)
        if hid:
            self.active.remove(hid)
        if children:
            node["children"] = children
        return node

    def convert_surface(self, item_name, surface_id):
        root = self.source["Data"]["RootChunk"]
        if root.get("$type") != "inkWidgetLibraryResource":
            raise ValueError("Expected inkWidgetLibraryResource")
        item = next((it for it in root.get("libraryItems", [])
                     if scalar(it.get("name")) == item_name), None)
        if item is None:
            raise ValueError("No library item: " + item_name)
        inst = item.get("package", {}).get("Data", {}).get("File", {}).get("RootChunk", {})
        node = self.convert_node(inst.get("rootWidget"))
        if node is None:
            raise ValueError("Unresolved rootWidget: " + item_name)
        deps = sorted({val for ref in root.get("externalDependenciesForInternalItems", [])
                       if (val := depot(ref))})
        animation = depot(root.get("animationLibraryResRef"))
        if animation:
            deps = sorted(set(deps + [animation]))
        surface = {
            "version": 1, "surface_id": surface_id, "source": "redengine-ink",
            "visible": False, "modal": False, "z_order": 200,
            "title": item_name, "subtitle": "", "style_tags": ["ink-source-import"],
            "metrics": {
                "ink_library_item": item_name,
                "ink_source_resolution": inst.get("rootResolution", ""),
                "ink_animation_resource": animation,
                "ink_dependency_count": len(deps),
                "ink_layout_support": "metadata_only",
            },
            "components": [node],
        }
        report = {
            "schema": "newviso.ink.import.report.v1",
            "source": self.source.get("Header", {}).get("ArchiveFileName", ""),
            "item": item_name, "widget_count": self.count,
            "widget_types": dict(sorted(self.types.items())),
            "unmapped_widget_types": dict(sorted(self.unsupported.items())),
            "unresolved_handles": sorted(self.missing_handles),
            "dependencies": deps,
            "source_animation_ref": animation,
            "verified_visual_equivalence": False,
            "verified_behavioral_equivalence": False,
            "unimplemented": [
                "exact Ink layout and constraints",
                "atlas XBM decode, UV sampling, material/shader blending",
                "font family and localized string binding",
                "game controller state and action dispatch",
                "Ink animation playback and interpolator targets",
                "reference screenshot pixel comparison",
            ],
        }
        return surface, report


def export_animation(source):
    root = source["Data"]["RootChunk"]
    if root.get("$type") != "inkanimAnimationLibraryResource":
        raise ValueError("Expected inkanimAnimationLibraryResource")
    clips = []
    counts = Counter()
    for ref in root.get("sequences", []):
        seq = ref.get("Data", {})
        definitions = seq.get("definitions", [])
        targets = seq.get("targets", [])
        tracks = []
        for idx, definition in enumerate(definitions):
            target = targets[idx].get("Data", {}) if idx < len(targets) else {}
            body = definition.get("Data", {})
            interpolators = [v.get("Data", {}) for v in body.get("interpolators", [])]
            counts.update(v.get("$type", "") for v in interpolators)
            tracks.append({
                "target_path": target.get("path", []),
                "interpolators": interpolators,
                "events": body.get("events", []),
            })
        clips.append({"name": scalar(seq.get("name")), "tracks": tracks})
    return {
        "schema": "newviso.ink.animation_source.v1",
        "clips": clips, "interpolator_types": dict(sorted(counts.items())),
        "runtime_evaluation_ready": False,
    }


def export_atlas(source):
    root = source["Data"]["RootChunk"]
    if root.get("$type") != "inkTextureAtlas":
        raise ValueError("Expected inkTextureAtlas")
    parts, textures = [], []
    for idx, slot in enumerate(root.get("slots", {}).get("Elements", [])):
        textures.append(depot(slot.get("texture")))
        for part in slot.get("parts", []):
            uv = part.get("clippingRectInUVCoords", {})
            parts.append({
                "name": scalar(part.get("partName")), "slot_index": idx,
                "uv_rect": [uv.get("Left", 0), uv.get("Top", 0),
                            uv.get("Right", 1), uv.get("Bottom", 1)],
            })
    return {
        "schema": "newviso.ink.atlas_source.v1",
        "parts": parts, "textures": textures, "runtime_texture_ready": False,
    }


def save(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--widget", required=True, type=Path)
    parser.add_argument("--item", default="Root")
    parser.add_argument("--surface-id", required=True)
    parser.add_argument("--animations", type=Path)
    parser.add_argument("--atlas", type=Path)
    parser.add_argument("--output-dir", required=True, type=Path)
    args = parser.parse_args()
    if not args.surface_id.strip() or len(args.surface_id) > 180:
        parser.error("invalid surface id")
    source = load_resource(args.widget)
    surface, report = InkImporter(source).convert_surface(args.item, args.surface_id)
    report["source_sha256"] = hashlib.sha256(args.widget.read_bytes()).hexdigest()
    stem = args.surface_id.replace("/", "_").replace("\\", "_")
    out = args.output_dir
    save(out / (stem + ".surface.json"), surface)
    if args.animations:
        animation = export_animation(load_resource(args.animations))
        save(out / (stem + ".animations.json"), animation)
        report["animation_clips"] = len(animation["clips"])
        report["animation_interpolators"] = animation["interpolator_types"]
    if args.atlas:
        atlas = export_atlas(load_resource(args.atlas))
        save(out / (stem + ".atlas.json"), atlas)
        report["atlas_parts"] = len(atlas["parts"])
    save(out / (stem + ".report.json"), report)
    print(json.dumps({
        "widgets": report["widget_count"], "unmapped_types": report["unmapped_widget_types"],
        "missing_handles": len(report["unresolved_handles"]),
        "animation_clips": report.get("animation_clips", 0),
        "atlas_parts": report.get("atlas_parts", 0),
        "visual_parity": False,
    }, ensure_ascii=False))


if __name__ == "__main__":
    main()
