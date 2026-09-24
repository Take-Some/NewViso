#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any


SCHEMA = "newviso.placements.v1"
SCENE_SCHEMA = "newviso.scene.v1"


def require_vec(value: Any, length: int, label: str) -> list[float]:
    if not isinstance(value, list) or len(value) != length:
        raise ValueError(f"{label} must contain {length} numeric values")
    out = [float(v) for v in value]
    if not all(v == v and abs(v) != float("inf") for v in out):
        raise ValueError(f"{label} must contain finite values")
    return out


def normalize_placement(record: dict[str, Any], index: int) -> dict[str, Any]:
    name = str(record.get("name") or f"Placement_{index:05d}")
    transform = record.get("transform") or {}
    position = require_vec(transform.get("position", [0, 0, 0]), 3, f"{name}.position")
    rotation = require_vec(
        transform.get("rotation_degrees", [0, 0, 0]),
        3,
        f"{name}.rotation_degrees",
    )
    scale = require_vec(transform.get("scale", [1, 1, 1]), 3, f"{name}.scale")

    mesh = record.get("mesh") or {}
    primitive = mesh.get("primitive")
    asset = mesh.get("asset")
    if primitive is None and asset is None:
        raise ValueError(f"{name}.mesh requires primitive or asset")

    entity: dict[str, Any] = {
        "kind": "mesh",
        "name": name,
        "transform": {
            "position": position,
            "rotation_degrees": rotation,
            "scale": scale,
        },
        "mesh": {},
    }

    if primitive is not None:
        entity["mesh"]["primitive"] = str(primitive)
        material = record.get("material") or {}
        entity["material"] = {
            "base_color": require_vec(
                material.get("base_color", [0.5, 0.5, 0.5, 1.0]),
                4,
                f"{name}.material.base_color",
            )
        }
    if asset is not None:
        entity["mesh"]["asset"] = str(asset)

    if "mobility" in record:
        entity["mobility"] = str(record["mobility"])

    collider = record.get("collider")
    if isinstance(collider, dict):
        entity["collider"] = {"solid": bool(collider.get("solid", False))}

    lod = record.get("lod")
    if isinstance(lod, dict):
        entity["lod"] = {
            key: float(value)
            for key, value in lod.items()
            if key in {"visible_distance", "stream_distance", "fade_range"}
        }

    visibility = record.get("visibility")
    if isinstance(visibility, dict):
        entity["visibility"] = {
            str(key): bool(value) for key, value in visibility.items()
        }

    return entity


def build_scene(source: dict[str, Any]) -> dict[str, Any]:
    if source.get("schema") != SCHEMA:
        raise ValueError(f"expected schema {SCHEMA!r}")

    camera = source.get("camera") or {}
    camera_position = require_vec(camera.get("position", [0, 1.7, 20]), 3, "camera.position")
    camera_target = require_vec(camera.get("target", [0, 1.7, 0]), 3, "camera.target")
    camera_up = require_vec(camera.get("up", [0, 1, 0]), 3, "camera.up")

    entities: list[dict[str, Any]] = [
        {
            "kind": "camera",
            "name": str(camera.get("name") or "ProjectCamera"),
            "transform": {
                "position": camera_position,
                "target": camera_target,
                "up": camera_up,
            },
            "camera": {
                "projection": "perspective",
                "fov_y_degrees": float(camera.get("fov_y_degrees", 72.0)),
                "near": float(camera.get("near", 0.08)),
                "far": float(camera.get("far", 600.0)),
            },
        }
    ]

    placements = source.get("placements")
    if not isinstance(placements, list):
        raise ValueError("placements must be an array")

    entities.extend(
        normalize_placement(record, index)
        for index, record in enumerate(placements)
        if isinstance(record, dict)
    )

    return {
        "schema": SCENE_SCHEMA,
        "version": 1,
        "title": str(source.get("title") or "Imported Map"),
        "entities": entities,
    }


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Convert neutral project-owned placement data into newviso.scene.v1"
    )
    parser.add_argument("input", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()

    source = json.loads(args.input.read_text(encoding="utf-8"))
    scene = build_scene(source)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(
        json.dumps(scene, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )
    print(
        f"wrote {args.output} entities={len(scene['entities'])} "
        f"placements={len(scene['entities']) - 1}"
    )


if __name__ == "__main__":
    main()
