#!/usr/bin/env python3
"""Import installed camera/weapon tuning, keeping XML and a small script projection.

Run from NorthStar/NewViso. Raw archive data stays in Tools/tmp; reusable catalog
data lives in Shared. The reference implementation is read from --source-root.
"""
from __future__ import annotations
import argparse, copy, hashlib, json, re, subprocess
from pathlib import Path
import xml.etree.ElementTree as ET

IMPORT_CORRECTIONS = []

def vector_number(raw: str) -> float:
    try: return float(raw)
    except ValueError:
        # Some installed Fx vectors contain an appended redundant decimal tail.
        match = re.fullmatch(r"([-+]?\d*\.\d+)\.0+", raw)
        if not match: raise
        IMPORT_CORRECTIONS.append(raw)
        return float(match[1])

def joaat(value: str) -> str:
    h = 0
    for c in value.lower().encode("utf-8"):
        h = (h + c) & 0xffffffff
        h = (h + (h << 10)) & 0xffffffff
        h ^= h >> 6
    h = (h + (h << 3)) & 0xffffffff
    h ^= h >> 11
    h = (h + (h << 15)) & 0xffffffff
    return f"hash_{h:08X}"

def value(node: ET.Element):
    if len(node):
        if all(c.tag == "Item" for c in node):
            return [value(c) for c in node]
        return {c.tag: value(c) for c in node}
    if "value" in node.attrib:
        raw = node.get("value", "")
        if raw in ("true", "false"):
            return raw == "true"
        try: return float(raw) if "." in raw else int(raw)
        except ValueError: return raw
    if "x" in node.attrib:
        return [vector_number(node.get(k, "0")) for k in ("x", "y", "z", "w") if k in node.attrib]
    return (node.text or "").strip()

WEAPON_FIELDS = {
    "Name", "FireType", "ClipSize", "AccuracySpread", "AccurateModeAccuracyModifier",
    "RunAndGunAccuracyModifier", "RecoilAccuracyMax", "RecoilErrorTime", "RecoilRecoveryRate",
    "Damage", "Force", "WeaponRange", "DamageFallOffRangeMin", "DamageFallOffRangeMax",
    "DamageFallOffModifier", "TimeBetweenShots", "TimeLeftBetweenShotsWhereShouldFireIsCached",
    "ReloadTimeSP", "AnimReloadRate", "BulletsInBatch", "BatchSpread", "WeaponFlags",
    "CameraFov", "FirstPersonAimFovMin", "FirstPersonAimFovMax", "FirstPersonScopeFov",
    "FirstPersonScopeAttachmentFov", "RecoilShakeHash", "RecoilShakeHashFirstPerson",
    "MinTimeBetweenRecoilShakes", "RecoilShakeAmplitude", "IkRecoilDisplacement",
    "IkRecoilDisplacementScope", "IkRecoilDisplacementScaleBackward", "IkRecoilDisplacementScaleVertical",
    "FirstPersonRNGOffset", "FirstPersonRNGRotationOffset", "FirstPersonLTOffset",
    "FirstPersonLTRotationOffset", "FirstPersonScopeOffset", "FirstPersonScopeRotationOffset", "Fx",
}

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--northstar-root", type=Path, default=Path(__file__).resolve().parents[2])
    ap.add_argument("--gta-root", type=Path, default=Path(r"C:\Games\Grand Theft Auto V"))
    ap.add_argument("--source-root", type=Path, default=Path(r"C:\Games\gta v source code\GTAV Source"))
    args = ap.parse_args()
    root = args.northstar_root
    cache = root / "Tools/tmp/weapon-camera-port"
    cache.mkdir(parents=True, exist_ok=True)
    cli = root / "Tools/rsc7-cli/bin/rsc7.exe"
    archive = args.gta_root / "update/update.rpf"
    schema = args.source_root / "src/dev_ng/game/camera/system/CameraMetadata.psc"
    names = set(re.findall(r'(?:name|type|base)="([^"]+)"', schema.read_text(encoding="utf-8")))
    names.update(n.removeprefix("m_") for n in list(names))
    names_file = cache / "camera_names.txt"
    names_file.write_text("\n".join(sorted(names)), encoding="utf-8")
    def run(*command):
        p = subprocess.run([str(cli), "--no-update-check", "--keys", str(root / "Tools/rsc7-cli/keys"), *map(str, command)], capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=60)
        if p.returncode: raise RuntimeError(p.stderr + p.stdout)
    camera_raw = cache / "x64/data/metadata/cameras.ymt"
    weapon_raw = cache / "common/data/ai/weapons.meta"
    camera_xml = cache / "cameras.xml"
    if not camera_raw.exists(): run("extract", archive, "x64/data/metadata/cameras.ymt", "-o", cache)
    if not weapon_raw.exists(): run("extract", archive, "common/data/ai/weapons.meta", "-o", cache)
    if not camera_xml.exists(): run("resource", "dump", camera_raw, "--names", names_file, "-o", camera_xml)
    camera_tree = ET.parse(camera_xml).getroot()
    camera_nodes = camera_tree.findall("./MetadataList/Item")
    nodes = {n.findtext("Name"): n for n in camera_nodes if n.findtext("Name")}
    fps = next(n for n in camera_nodes if n.get("type") == "camFirstPersonShooterCameraMetadata")
    weapon_nodes = ET.parse(weapon_raw).getroot().findall(".//Item[@type='CWeaponInfo']")
    weapons = {n.findtext("Name"): {c.tag: value(c) for c in n if c.tag in WEAPON_FIELDS} for n in weapon_nodes if n.findtext("Name")}
    director_node = next(n for n in camera_tree.findall("./DirectorList/Item")
                         if n.get("type") == "camGameplayDirectorMetadata"
                         and n.findtext("FirstPersonShooterCameraRef") == fps.findtext("Name"))
    director_fields = {"MaxRecoilShakeInstances", "RecoilShakeFirstPersonShootingAbilityLimits",
        "RecoilShakeAmplitudeScalingAtFirstPersonShootingAbilityLimits", "RecoilShakeAccuracyLimits",
        "RecoilShakeAmplitudeScalingAtAccuracyLimits", "MaxKillEffectShakeInstances",
        "KillEffectShakeRef", "VehicleImpactShakeRef", "DeathFailIntroEffectShakeRef",
        "DeathFailOutroEffectShakeRef", "AimCameraInterpolateDuration", "FirstPersonAimCameraInterpolateDuration"}
    director = {c.tag: value(c) for c in director_node if c.tag in director_fields}
    pending = [fps.findtext("Name")]
    pending += [v for k, v in director.items() if k.endswith("Ref") and isinstance(v, str)]
    for w in weapons.values():
        pending += [joaat(str(w[k])) for k in ("RecoilShakeHash", "RecoilShakeHashFirstPerson") if w.get(k)]
    selected = {}
    while pending:
        name = pending.pop()
        if name in selected or name not in nodes: continue
        n = nodes[name]
        selected[name] = {"type": n.get("type"), **value(n)}
        pending.extend(c.text.strip() for c in n.iter() if c is not n and c.text and c.text.strip() in nodes and c.tag != "Name")
    unresolved = sorted({joaat(str(w[k])) for w in weapons.values() for k in ("RecoilShakeHash", "RecoilShakeHashFirstPerson") if w.get(k)} - selected.keys())
    # Keep authored camera configuration in XML; this is generated content data.
    out = ET.Element("WeaponCameraReference", schema="newviso.weapon-camera.reference.v1")
    for p in (camera_raw, weapon_raw, schema):
        ET.SubElement(out, "Source", path=str(p), sha256=hashlib.sha256(p.read_bytes()).hexdigest())
    cameras = ET.SubElement(out, "CameraMetadata")
    for name in sorted(selected): cameras.append(copy.deepcopy(nodes[name]))
    ET.SubElement(out, "Directors").append(copy.deepcopy(director_node))
    weapon_xml = ET.SubElement(out, "Weapons")
    for n in weapon_nodes: weapon_xml.append(copy.deepcopy(n))
    ET.indent(out, space="  ")
    target = root / "Shared/Content/config/engine/weapon_camera.reference.xml"
    target.write_text('<?xml version="1.0" encoding="UTF-8"?>\n' + ET.tostring(out, encoding="unicode") + "\n", encoding="utf-8")
    catalog = {"schema": "newviso.weapon-camera.reference.v1", "fps": fps.findtext("Name"), "director": director,
        "standard_accuracy_range": 120.0, "accuracy_modifier_min": 1.0, "accuracy_modifier_max": 1.0,
        "camera_sha256": hashlib.sha256(camera_raw.read_bytes()).hexdigest(), "weapon_sha256": hashlib.sha256(weapon_raw.read_bytes()).hexdigest(),
        "source_vector_corrections": sorted(set(IMPORT_CORRECTIONS)), "objects": selected, "weapons": weapons, "unresolved_recoil_refs": unresolved}
    script = root / "Shared/Content/scripts/newviso/gameplay/weapon_camera_reference.ysc"
    script.write_text("// Generated by NewViso/scripts/import_weapon_camera_reference.py.\nexport const WEAPON_CAMERA_REFERENCE: any = " + json.dumps(catalog, separators=(",", ":")) + ";\n", encoding="utf-8")
    print(json.dumps({"cameras": len(selected), "weapons": len(weapons), "unresolved_recoil_refs": unresolved, "source_vector_corrections": sorted(set(IMPORT_CORRECTIONS)), "script_bytes": script.stat().st_size, "xml": str(target)}))

if __name__ == "__main__": main()
