#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import math
from collections import defaultdict
from pathlib import Path
from typing import Any

SOURCE_SCHEMA = "newviso.placements.v1"
MAP_INDEX_SCHEMA = "newviso.map.index.v1"
MAP_CELL_SCHEMA = "newviso.map.cell.v1"


def finite_vec3(value: Any, label: str) -> list[float]:
    if not isinstance(value, list) or len(value) != 3:
        raise ValueError(f"{label} must contain 3 numeric values")
    out = [float(v) for v in value]
    if not all(math.isfinite(v) for v in out):
        raise ValueError(f"{label} must contain finite values")
    return out


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Partition neutral NewViso placements into independently addressable map cells"
    )
    parser.add_argument("input", type=Path)
    parser.add_argument("output_dir", type=Path)
    parser.add_argument("--map-id", required=True)
    parser.add_argument("--cell-size", type=float, default=32.0)
    parser.add_argument("--origin", default="0,0,0")
    args = parser.parse_args()

    if not math.isfinite(args.cell_size) or args.cell_size <= 0:
        raise SystemExit("--cell-size must be finite and > 0")

    origin = [float(v.strip()) for v in args.origin.split(",")]
    if len(origin) != 3 or not all(math.isfinite(v) for v in origin):
        raise SystemExit("--origin must be x,y,z with finite values")

    source = json.loads(args.input.read_text(encoding="utf-8"))
    if source.get("schema") != SOURCE_SCHEMA:
        raise SystemExit(f"expected {SOURCE_SCHEMA!r}")

    placements = source.get("placements")
    if not isinstance(placements, list):
        raise SystemExit("placements must be an array")

    cells: dict[tuple[int, int], list[dict[str, Any]]] = defaultdict(list)
    seen_ids: set[str] = set()

    for index, record in enumerate(placements):
        if not isinstance(record, dict):
            continue
        placement_id = str(record.get("id") or record.get("name") or f"placement_{index:06d}")
        if placement_id in seen_ids:
            raise SystemExit(f"duplicate placement id: {placement_id}")
        seen_ids.add(placement_id)

        transform = record.get("transform") or {}
        position = finite_vec3(transform.get("position", [0, 0, 0]), f"{placement_id}.position")
        rotation = finite_vec3(
            transform.get("rotation_degrees", [0, 0, 0]),
            f"{placement_id}.rotation_degrees",
        )
        scale = finite_vec3(transform.get("scale", [1, 1, 1]), f"{placement_id}.scale")

        coord = (
            math.floor((position[0] - origin[0]) / args.cell_size),
            math.floor((position[2] - origin[2]) / args.cell_size),
        )

        normalized = {
            "id": placement_id,
            "transform": {
                "position": position,
                "rotation_degrees": rotation,
                "scale": scale,
            },
            "mesh": record.get("mesh") or {},
            "material": record.get("material"),
            "collider": record.get("collider"),
            "mobility": record.get("mobility", "static"),
            "lod": record.get("lod"),
            "visibility": record.get("visibility"),
            "tags": record.get("tags", []),
            "definition_ref": record.get("definition_ref"),
        }
        cells[coord].append(normalized)

    output = args.output_dir
    cells_dir = output / "cells"
    cells_dir.mkdir(parents=True, exist_ok=True)

    cell_refs: list[dict[str, Any]] = []
    for x, z in sorted(cells):
        records = sorted(cells[(x, z)], key=lambda record: record["id"])
        cell_path = cells_dir / f"{x}_{z}.json"
        cell_payload = {
            "schema": MAP_CELL_SCHEMA,
            "coord": {"x": x, "z": z},
            "placements": records,
            "metadata": {
                "source": args.input.as_posix(),
            },
        }
        cell_path.write_text(
            json.dumps(cell_payload, indent=2, ensure_ascii=False) + "\n",
            encoding="utf-8",
        )
        cell_refs.append(
            {
                "coord": {"x": x, "z": z},
                "path": f"cells/{x}_{z}.json",
                "required": True,
                "placement_count": len(records),
            }
        )

    index_payload = {
        "schema": MAP_INDEX_SCHEMA,
        "map_id": args.map_id,
        "origin": origin,
        "cell_size": args.cell_size,
        "cells": cell_refs,
        "metadata": {
            "title": str(source.get("title") or args.map_id),
            "camera": source.get("camera"),
            "placement_count": len(seen_ids),
            "source": args.input.as_posix(),
        },
    }
    (output / "index.json").write_text(
        json.dumps(index_payload, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )

    print(
        f"map={args.map_id} cells={len(cell_refs)} placements={len(seen_ids)} "
        f"cell_size={args.cell_size:g}"
    )


if __name__ == "__main__":
    main()
