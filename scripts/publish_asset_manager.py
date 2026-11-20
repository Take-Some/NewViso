#!/usr/bin/env python3
"""ABI-gated, rollback-safe StarVault publisher; never synthesizes provider DLLs."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
ENGINE = ROOT / "NewViso"
PATTERN = re.compile(r"starVault-assetManager-(\d+\.\d+\.\d+)-release\.dll")
PROVIDER_ID = "engine.assets.starvault"


def sha256(file):
    return hashlib.sha256(file.read_bytes()).hexdigest()


def version(text):
    if not re.fullmatch(r"\d+\.\d+\.\d+", text):
        raise ValueError(f"invalid release version: {text}")
    return tuple(map(int, text.split(".")))


def probe(exe, dll, ver):
    p = subprocess.run([str(exe), str(dll), PROVIDER_ID, ver],
                       capture_output=True, text=True, timeout=35)
    if p.returncode:
        raise RuntimeError(f"ABI probe failed: {p.stderr.strip()}")
    result = json.loads(p.stdout)
    if not result["abi_current"] or not result["descriptor_v2"]:
        raise RuntimeError("current provider ABI and descriptor v2 are required")
    return result


def capabilities(result):
    return {c["id"]: c for c in result["capabilities"]}


def methods(result):
    cap = capabilities(result).get("asset_manager.api")
    if not cap:
        raise RuntimeError("asset_manager.api is absent")
    declared = cap["description"]["methods"]
    names = {x["name"]: x for x in declared}
    if len(names) != len(declared):
        raise RuntimeError("duplicate or invalid service methods")
    return names


def ensure_compatible(old, new):
    if new["provider_id"] != old["provider_id"]:
        raise RuntimeError("provider identity changed")
    if version(new["provider_version"]) <= version(old["provider_version"]):
        raise RuntimeError("candidate is not a newer release")
    oc, nc = capabilities(old), capabilities(new)
    if not oc.keys() <= nc.keys():
        raise RuntimeError(f"missing capabilities: {sorted(oc.keys()-nc.keys())}")
    for key in oc:
        if nc[key]["version"] < oc[key]["version"] or nc[key]["role"] != oc[key]["role"] or nc[key]["kind"] != oc[key]["kind"]:
            raise RuntimeError(f"capability contract regression: {key}")
    om, nm = methods(old), methods(new)
    if not om.keys() <= nm.keys():
        raise RuntimeError(f"missing service methods: {sorted(om.keys()-nm.keys())}")
    for key, previous in om.items():
        for field in ("payload", "returns"):
            if previous.get(field) != nm[key].get(field):
                raise RuntimeError(f"changed method contract: {key}.{field}")


def smoke(project, dll):
    p = subprocess.run([sys.executable, str(ENGINE / "scripts/profile_live_runtime.py"),
                       "--project", str(project), "--frames", "120", "--timeout", "45",
                       "--warmup", "10", "--min-gameplay-samples", "15"],
                       cwd=ENGINE, text=True, capture_output=True, timeout=70)
    reports = []
    for line in p.stdout.splitlines():
        try:
            value = json.loads(line)
        except ValueError:
            continue
        if isinstance(value, dict) and "render_samples" in value:
            reports.append(value)
    if not reports:
        raise RuntimeError(f"no runtime smoke report: {p.stderr[-600:]}")
    r = reports[-1]
    if r.get("exit_code") != 0 or r.get("timed_out") or r.get("render_samples", 0) < 10:
        raise RuntimeError(f"runtime startup failed: {r}")
    if r.get("streaming", {}).get("resident", 0) < 1 or r["streaming"].get("failed", 0):
        raise RuntimeError(f"resource residency failed: {r}")
    log = Path(r["report"]).with_suffix(".stderr.log").read_text(encoding="utf8", errors="replace")
    if "asset gateway validated" not in log or dll.name not in log:
        raise RuntimeError("updated provider was not validated and loaded")
    return {"report": r["report"], "resident": r["streaming"]["resident"],
            "map_ready": r.get("map_ready"), "gameplay_samples": r.get("steady_gameplay_samples")}


def publish(args):
    runtime, checker = args.runtime.resolve(), args.probe.resolve()
    if not checker.is_file():
        raise RuntimeError(f"missing ABI probe executable: {checker}")
    found = []
    for file in runtime.glob("starVault-assetManager-*-release.dll"):
        match = PATTERN.fullmatch(file.name)
        if match:
            found.append((match.group(1), file))
    if len(found) != 1:
        raise RuntimeError(f"expected exactly one installed StarVault; found {found}")
    previous_version, old_dll = found[0]
    old = probe(checker, old_dll, previous_version)
    old_hash = sha256(old_dll)
    if not args.candidate:
        return {"status": "audit", "installed": str(old_dll), "sha256": old_hash,
                "methods": len(methods(old)), "capabilities": len(capabilities(old))}
    candidate = args.candidate.resolve()
    match = PATTERN.fullmatch(candidate.name)
    if not candidate.is_file() or not match:
        raise RuntimeError("candidate DLL path/name is invalid")
    candidate_version = match.group(1)
    new = probe(checker, candidate, candidate_version)
    ensure_compatible(old, new)
    candidate_hash = sha256(candidate)
    if candidate_hash == old_hash:
        raise RuntimeError("candidate is byte-identical to old DLL")
    receipt = {"previous": previous_version, "new": candidate_version,
               "old_sha256": old_hash, "new_sha256": candidate_hash,
               "methods": len(methods(new)), "capabilities": len(capabilities(new))}
    if not args.install:
        return {"status": "validated_not_installed", **receipt}
    if not args.project or not args.project.is_dir():
        raise RuntimeError("--install requires --project for post-install smoke test")
    if os.name == "nt":
        process = subprocess.run(
            ["tasklist", "/FI", "IMAGENAME eq newviso.exe", "/FO", "CSV", "/NH"],
            capture_output=True, text=True, timeout=15,
        )
        if process.returncode != 0:
            raise RuntimeError("cannot confirm whether the game is running")
        if any('"newviso.exe"' in line.lower() for line in process.stdout.splitlines()):
            raise RuntimeError("NewViso is running: stop the game before replacing DLLs")
    if candidate.parent == runtime:
        raise RuntimeError("candidate must not be inside the live pluginsRuntime directory")
    dest = runtime / candidate.name
    if dest.exists():
        raise RuntimeError(f"destination exists: {dest}")
    backups = ENGINE / "cache" / "provider-backups"
    backups.mkdir(parents=True, exist_ok=True)
    backup = backups / (old_dll.stem + "-" + old_hash[:16] + ".dll")
    if backup.exists() and sha256(backup) != old_hash:
        raise RuntimeError("backup filename collision with different bytes")
    if not backup.exists():
        shutil.copy2(old_dll, backup)
    stage = runtime / (".new-asset-" + candidate_hash[:16] + ".dll")
    shutil.copy2(candidate, stage)
    if sha256(stage) != candidate_hash:
        raise RuntimeError("staging SHA-256 mismatch")
    os.replace(old_dll, backup)
    try:
        os.replace(stage, dest)
        probe(checker, dest, candidate_version)
        receipt["smoke"] = smoke(args.project, dest)
        receipt["backup"] = str(backup)
        (backups / ("starvault-" + candidate_version + ".json")).write_text(
            json.dumps(receipt, indent=2), encoding="utf8")
        return {"status": "installed", **receipt}
    except BaseException:
        dest.unlink(missing_ok=True)
        os.replace(backup, old_dll)
        raise
    finally:
        stage.unlink(missing_ok=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime", type=Path, default=ROOT / "pluginsRuntime")
    parser.add_argument("--probe", type=Path, default=ENGINE / "target/release/asset_provider_probe.exe")
    parser.add_argument("--candidate", type=Path)
    parser.add_argument("--project", type=Path)
    parser.add_argument("--install", action="store_true")
    args = parser.parse_args()
    if args.install and not args.candidate:
        parser.error("--install requires --candidate")
    try:
        print(json.dumps(publish(args), indent=2, ensure_ascii=False))
    except Exception as e:
        print("ASSET_MANAGER_PUBLISH_BLOCKED:", e, file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
