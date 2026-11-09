"""Inventory reference units and explicit native mappings; never infer behavioural parity."""
from pathlib import Path
import argparse
import hashlib
import json

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_REFERENCE = Path(r"C:\Games\gta v source code\GTAV Source")
CORE = "crates/newviso-endorphin/src/"
HOST = "crates/newviso-runtime/src/"
MAPPINGS = {
    "TaskNM.cpp": [CORE+"authority.rs", HOST+"application_endorphin.rs"],
    "TaskNMBalance.cpp": [CORE+"balance.rs", CORE+"support.rs", CORE+"stepping.rs"],
    "TaskNMBrace.cpp": [CORE+"protection.rs", HOST+"application_endorphin/protection/physical.rs"],
    "TaskNMHighFall.cpp": [CORE+"recovery.rs", HOST+"application_peds/physics/animation.rs"],
    "TaskNMFallDown.cpp": [CORE+"reactions.rs", HOST+"application_endorphin/ragdoll.rs"],
    "TaskNMFlinch.cpp": [CORE+"reactions.rs", HOST+"application_peds/physics/wounds.rs"],
    "TaskNMShot.cpp": [CORE+"anatomy.rs", HOST+"application_endorphin/damage.rs"],
    "TaskNMRelax.cpp": [HOST+"application_endorphin/control.rs"],
    "TaskBlendFromNM.cpp": [CORE+"recovery_match.rs", HOST+"application_endorphin/getup.rs",
        "crates/newviso-scene/src/first_scene/animation_skinning/pose_layers/recovery.rs"],
    "TaskGetUp.cpp": [CORE+"recovery_support.rs", HOST+"application_peds/physics/recovery_support.rs", CORE+"recovery_match.rs", CORE+"recovery_placement.rs", HOST+"application_endorphin/getup.rs",
        HOST+"application_peds/physics/recovery.rs", HOST+"application_physics/queries/getup.rs",
        HOST+"application_physics/queries/recovery.rs", HOST+"application_peds/physics/animation.rs",
        "crates/newviso-scene/src/first_scene/animation_skinning/pose_layers/recovery.rs"],
    "NmRsCBU_AnimPose.cpp": [CORE+"anatomy.rs"],
    "NmRsCBU_BodyBalance.cpp": [CORE+"balance.rs", CORE+"support.rs"],
    "NmRsCBU_DynamicBalancer.cpp": [CORE+"stepping.rs"],
    "NmRsCBU_BraceForImpact.cpp": [CORE+"protection.rs"],
    "NmRsCBU_Catchfall.cpp": [CORE+"protection.rs"],
    "NmRsCBU_HighFall.cpp": [CORE+"recovery.rs", HOST+"application_endorphin/control.rs"],
    "NmRsCBU_ShotControlStiffness.cpp": [CORE+"anatomy.rs"],
    "NmRsCBU_ShotInjuredLimb.cpp": [CORE+"anatomy.rs"],
}
LIMITS = [
    "A partial mapping identifies related native responsibilities, not a source port or accepted parity.",
    "Selected Physics and NaturalMotion implementation scopes plus TaskGetUp.cpp; headers and other integration directories are not a complete engine inventory.",
    "Native behaviour units include helper and job implementation files; their count is not a behaviour count.",
    "No original-runtime golden trace has been compared. No unit is marked exact-parity verified.",
    "Authored tuning, animation assets, topology, contacts and networking need separate parity acceptance.",
    "Sustained physical standing, physical locomotion and physical LOD remain open.",
]

def inventory(reference):
    scopes = [
        ("game_task", "src/dev_ng/game/task/Physics", "Task*.cpp"),
        ("recovery_task", "src/dev_ng/game/task/Movement", "TaskGetUp.cpp"),
        ("native_behaviour_unit", "src/dev_ng/rage/naturalmotion/src/rockstar", "NmRsCBU_*.cpp"),
        ("native_support_unit", "src/dev_ng/rage/naturalmotion/src/rockstar", "*.cpp"),
    ]
    mappings = {name.casefold(): files for name, files in MAPPINGS.items()}
    units = []
    seen = set()
    for kind, relative, pattern in scopes:
        directory = reference/relative
        if not directory.is_dir():
            raise RuntimeError("Missing reference scope: "+str(directory))
        for path in sorted(directory.glob(pattern), key=lambda p:p.name.lower()):
            canonical = str(path.resolve()).casefold()
            if canonical in seen:
                continue
            seen.add(canonical)
            if kind == "native_support_unit" and path.name.casefold().startswith("nmrscbu_"):
                continue
            mapping = mappings.get(path.name.casefold(), [])
            for native in mapping:
                if not (ROOT/native).is_file():
                    raise RuntimeError("Missing native mapping: "+native)
            units.append({
                "source": path.relative_to(reference).as_posix(),
                "kind": kind,
                "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                "status": "mapped_partial" if mapping else "unmapped",
                "native_mapping": mapping,
                "exact_parity_verified": False,
            })
    ids = [u["source"] for u in units]
    if len(ids) != len(set(ids)):
        raise RuntimeError("Duplicate reference unit")
    counts = {kind:sum(u["kind"] == kind for u in units) for kind,_,_ in scopes}
    return {
        "schema": "newviso.endorphin.reference-coverage.v1",
        "reference_root": str(reference.resolve()),
        "counts": counts,
        "mapped_partial": sum(u["status"] == "mapped_partial" for u in units),
        "exact_parity_verified": False,
        "scope_limits": LIMITS,
        "units": units,
    }

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference-root", type=Path, default=DEFAULT_REFERENCE)
    parser.add_argument("--output", type=Path, default=ROOT/"docs/endorphin-reference-coverage.json")
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    result = inventory(args.reference_root)
    if args.check:
        if json.loads(args.output.read_text(encoding="utf-8")) != result:
            raise RuntimeError("Reference inventory changed; review source hashes and mappings before refreshing.")
    else:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        temporary = args.output.with_suffix(args.output.suffix+".tmp")
        temporary.write_text(json.dumps(result, indent=2)+"\n", encoding="utf-8")
        temporary.replace(args.output)
    print(json.dumps({"inventory_complete":True,"counts":result["counts"],
        "mapped_partial":result["mapped_partial"],"exact_parity_verified":False,"check":args.check}))
if __name__ == "__main__":
    main()
