"""Inventory local reference files without copying source bodies.

Usage:
  python scripts/audit_vehicle_reference.py --source-root "<reference game directory>"
The report inventories files; it never treats a file as fully ported.
"""
from pathlib import Path
import argparse
import hashlib
import json
import re

ENGINE = Path(__file__).resolve().parents[1]
KINDS = ("vehicle", "car", "boat", "bike", "bmx", "plane", "heli", "train", "submarine", "trailer")


def collect(root):
    result = []
    for directory in ("Vehicles", "vehicleAi", "event", "network"):
        base = root / directory
        if not base.is_dir():
            continue
        for path in sorted(base.rglob("*")):
            if path.suffix.lower() not in (".cpp", ".h", ".hpp", ".inl"):
                continue
            raw = path.read_bytes()
            text = raw.decode("utf-8", errors="replace")
            if directory not in ("Vehicles", "vehicleAi") and not (
                any(kind in path.stem.lower() for kind in KINDS)
                or re.search(r"\b(?:CVehicle|CAutomobile|CBike|CTrain|CBoat|CPlane|CRotaryWingAircraft)\b", text)
            ):
                continue
            result.append({
                "path": path.relative_to(root).as_posix(),
                "area": directory,
                "lines": len(text.splitlines()),
                "bytes": len(raw),
                "sha256": hashlib.sha256(raw).hexdigest(),
                "review_status": "inventory_only",
            })
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-root", type=Path, required=True)
    parser.add_argument("--output", type=Path,
                        default=ENGINE / ".newviso/verification/vehicle-reference/source-inventory.json")
    args = parser.parse_args()
    root = args.source_root.resolve()
    nested = root / "GTAV Source/src/dev_ng/game"
    if nested.is_dir():
        root = nested
    if not (root / "Vehicles").is_dir():
        parser.error("source root must contain the Vehicles directory")
    files = collect(root)
    totals = {}
    for entry in files:
        total = totals.setdefault(entry["area"], {"files": 0, "lines": 0, "bytes": 0})
        for key in ("lines", "bytes"):
            total[key] += entry[key]
        total["files"] += 1
    report = {
        "schema": "newviso.vehicle.reference_inventory.v1",
        "source_root": str(root),
        "scope": "vehicles, vehicle AI, vehicle-related event and network declarations",
        "coverage_note": "File inventory is not behavioral parity. Reviewed behaviors and remaining work are in docs/vehicle-reference-port.md.",
        "totals": totals,
        "files": files,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"report": str(args.output), "totals": totals}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
