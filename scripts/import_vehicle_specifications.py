"""Translate local authored vehicle metadata into the native Shared XML catalog.

Only named, implemented capabilities execute. Source flag masks/names remain in
the catalog for later passes; retaining a flag is not implementing its behavior.
"""
from pathlib import Path
import argparse
import json
import xml.etree.ElementTree as ET

WORKSPACE = Path(__file__).resolve().parents[2]


def flag_mask(item, tag):
    raw = (item.findtext(tag) or "0").strip()
    value = int(raw, 16)
    if not 0 <= value <= 0xFFFFFFFF:
        raise ValueError(f"{tag} is outside u32: {raw}")
    return value


def specification(vehicle, handling):
    model = flag_mask(handling, "strModelFlags")
    control = flag_mask(handling, "strHandlingFlags")
    flags = (vehicle.findtext("flags") or "").split()
    kind = (vehicle.findtext("type") or "").strip()
    unpowered = kind in {"VEHICLE_TYPE_TRAILER", "VEHICLE_TYPE_BICYCLE"}
    electric = "FLAG_IS_ELECTRIC" in flags
    carries_petrol = kind == "VEHICLE_TYPE_TRAILER" and any(
        flag in flags for flag in ("FLAG_SPRAY_PETROL_BEFORE_EXPLOSION", "FLAG_EXPLODE_ON_CONTACT"))
    combustion = not unpowered and not electric
    steering = "all" if control & 0x80 else "rear" if control & 0x20 else "handbrake_rear" if control & 0x40 else "configured"
    return {
        "model_name": vehicle.findtext("modelName"),
        "handling_id": vehicle.findtext("handlingId"),
        "layout": vehicle.findtext("layout") or "",
        "vehicle_type": kind,
        "vehicle_class": vehicle.findtext("vehicleClass") or "",
        "source_model_flags": model,
        "source_handling_flags": control,
        "source_door_damage_flags": flag_mask(handling, "strDamageFlags"),
        "source_flags": flags,
        "powertrain": "none" if unpowered else "electric" if electric else "combustion",
        "carries_petrol": carries_petrol,
        "exhaust": combustion and not bool(model & 0x1000),
        "damage": {
            "indestructible": bool(model & 0x04000000),
            "bullet_proof": bool(control & 0x08000000),
            "tyres_can_burst": not bool(model & 0x02000000),
            "wheels_can_break": not bool(model & 0x40000000),
            "engine_damage": not unpowered,
            "petrol_tank_damage": combustion or carries_petrol,
            "engine_fires": combustion,
            "engine_misfires": combustion,
            # Source defaults allow leakage from the local player's vehicle;
            # gameplay can explicitly enable or disable leakage for other cars.
            "oil_leaks": "player_only" if combustion else "disabled",
            "petrol_leaks": "player_only" if combustion or carries_petrol else "disabled",
        },
        "controls": {"reverse": not bool(control & 0x200), "handbrake": not bool(control & 0x10), "steering": steering},
    }


def xml_value(parent, name, value):
    node = ET.SubElement(parent, name)
    if isinstance(value, dict):
        for key, child in value.items():
            xml_value(node, key, child)
    elif isinstance(value, list):
        node.set("type", "array")
        for child in value:
            xml_value(node, "item", child)
    elif value is None:
        node.set("null", "true")
    else:
        node.set("value", str(value).lower() if isinstance(value, bool) else str(value))
    return node


def build_catalog(handling_paths, vehicle_paths):
    handling = {}
    for path in handling_paths:
        for item in ET.parse(path).getroot().iter("Item"):
            name = item.findtext("handlingName")
            if name:
                handling[name.casefold()] = item
    models = {}
    unresolved = []
    for path in vehicle_paths:
        for item in ET.parse(path).getroot().iter("Item"):
            name = item.findtext("modelName")
            if not name:
                continue
            key = (item.findtext("handlingId") or "").casefold()
            if key not in handling:
                unresolved.append({"model": name, "handling_id": key, "source": str(path)})
                continue
            models[name.casefold()] = specification(item, handling[key])
    return {"schema": "newviso.vehicle.specifications.v1", "vehicles": [models[k] for k in sorted(models)]}, unresolved


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--handling", type=Path, action="append", required=True)
    parser.add_argument("--vehicles", type=Path, action="append", required=True)
    parser.add_argument("--output", type=Path, default=WORKSPACE / "Shared/Content/config/vehicles/specifications.xml")
    args = parser.parse_args()
    catalog, unresolved = build_catalog(args.handling, args.vehicles)
    if unresolved:
        raise ValueError("unresolved vehicle handling profiles: " + json.dumps(unresolved))
    root = ET.Element("vehicle_specifications", schema=catalog["schema"])
    xml_value(root, "vehicles", catalog["vehicles"])
    ET.indent(root, "  ")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    ET.ElementTree(root).write(args.output, encoding="utf-8", xml_declaration=True)
    print(json.dumps({"output": str(args.output), "vehicles": len(catalog["vehicles"]),
        "electric": sum(v["powertrain"] == "electric" for v in catalog["vehicles"]),
        "fuel_trailers": sum(v["carries_petrol"] for v in catalog["vehicles"]),
        "note": "Only damage/control capabilities mapped above are executable; other flags are retained metadata."}))


if __name__ == "__main__":
    main()
