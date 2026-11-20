#!/usr/bin/env python3
"""Transactional StarVault runtime publication with verified rollback."""
from pathlib import Path
import hashlib
import json
import os
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
CACHE = ROOT / "NewViso" / "cache"
LIVE = ROOT / "pluginsRuntime"
STAGE = CACHE / "starvault-runtime-stage-20261112"
CANDIDATE = CACHE / "starvault-runtime-candidate-20261112"
BACKUP = CACHE / "starvault-runtime-backup-20261112"
RECORD = CACHE / "starvault-install-20261112.json"
NEW_PROVIDER = "starVault-assetManager-3.8.1-release.dll"
OLD_PROVIDER = "starVault-assetManager-3.8.0-release.dll"
NEW_CODEC = "newengine-codec-listfile-0.1.1-release.dll"
OLD_CODEC = "newengine-codec-listfile-0.1.0-release.dll"

def sha(path):
    h = hashlib.sha256()
    with path.open("rb") as f:
        for part in iter(lambda: f.read(1024 * 1024), b""):
            h.update(part)
    return h.hexdigest()

def selected_files(root):
    return [root / NEW_PROVIDER, root / "codecs" / NEW_CODEC, *sorted((root / "formats").glob("*.dll"))]

def validate(root):
    if not root.is_dir():
        raise RuntimeError(f"missing runtime root: {root}")
    if (root / OLD_PROVIDER).exists() or (root / "codecs" / OLD_CODEC).exists():
        raise RuntimeError("old and new versions cannot coexist")
    if len(list(root.glob("starVault-assetManager-*.dll"))) != 1:
        raise RuntimeError("duplicate/missing asset manager")
    if len(list((root / "codecs").glob("newengine-codec-listfile-*.dll"))) != 1:
        raise RuntimeError("duplicate/missing listfile codec")
    if len(list((root / "formats").glob("*.dll"))) != 30:
        raise RuntimeError("expected 30 staged format DLLs")
    return {str(p.relative_to(root)).replace("\\", "/"): sha(p) for p in selected_files(root)}

def ensure_stopped():
    cmd = subprocess.run(["tasklist", "/FI", "IMAGENAME eq newviso.exe", "/FO", "CSV", "/NH"],
                         capture_output=True, text=True, errors="replace")
    if cmd.returncode != 0 or '"newviso.exe"' in cmd.stdout.lower():
        raise RuntimeError("newviso.exe still running or process check unavailable")

def save(data):
    tmp = RECORD.with_suffix(".json.writing")
    tmp.write_text(json.dumps(data, indent=2), encoding="utf-8")
    os.replace(tmp, RECORD)

def prepare():
    if CANDIDATE.exists() or BACKUP.exists():
        raise RuntimeError("existing transaction directory; inspect before publishing")
    if not LIVE.is_dir():
        raise RuntimeError("installed runtime absent")
    old_hashes = {"provider": sha(LIVE / OLD_PROVIDER), "codec": sha(LIVE / "codecs" / OLD_CODEC)}
    validate(STAGE)
    try:
        # Hardlink unchanged content (shader tools etc.), but NEVER overwrite a linked file.
        for source in LIVE.rglob("*"):
            if not source.is_file():
                continue
            rel = source.relative_to(LIVE)
            if rel.as_posix().lower() in {OLD_PROVIDER.lower(), ("codecs/" + OLD_CODEC).lower()}:
                continue
            dest = CANDIDATE / rel
            dest.parent.mkdir(parents=True, exist_ok=True)
            try:
                os.link(source, dest)
            except OSError:
                shutil.copy2(source, dest)
        for source in STAGE.rglob("*"):
            if not source.is_file():
                continue
            dest = CANDIDATE / source.relative_to(STAGE)
            dest.parent.mkdir(parents=True, exist_ok=True)
            if dest.exists():
                dest.unlink()
            shutil.copy2(source, dest)
        hashes = validate(CANDIDATE)
        if sha(LIVE / OLD_PROVIDER) != old_hashes["provider"]:
            raise RuntimeError("installed provider changed during preparation")
        save({"status": "prepared", "old": old_hashes, "new": hashes})
    except BaseException:
        # Leave candidate for diagnosis, never touch the running installation.
        raise
    print("PREPARED", len(hashes), "verified provider/codec/format DLL hashes")

def commit():
    ensure_stopped()
    record = json.loads(RECORD.read_text(encoding="utf-8"))
    if record["status"] != "prepared" or BACKUP.exists():
        raise RuntimeError("transaction is not ready")
    if sha(LIVE / OLD_PROVIDER) != record["old"]["provider"]:
        raise RuntimeError("old provider changed since prepare")
    if sha(LIVE / "codecs" / OLD_CODEC) != record["old"]["codec"]:
        raise RuntimeError("old codec changed since prepare")
    if validate(CANDIDATE) != record["new"]:
        raise RuntimeError("candidate SHA256 mismatch")
    os.replace(LIVE, BACKUP)
    try:
        os.replace(CANDIDATE, LIVE)
        if validate(LIVE) != record["new"]:
            raise RuntimeError("installed SHA256 mismatch")
    except BaseException:
        if LIVE.exists():
            recovery = CACHE / "starvault-recovery-failed-install-20261112"
            if recovery.exists():
                raise RuntimeError("recovery destination already exists")
            os.replace(LIVE, recovery)
        os.replace(BACKUP, LIVE)
        raise
    record["status"] = "installed"
    save(record)
    print("INSTALLED", NEW_PROVIDER, "backup", BACKUP)

def rollback():
    ensure_stopped()
    record = json.loads(RECORD.read_text(encoding="utf-8"))
    if record["status"] != "installed" or not BACKUP.is_dir():
        raise RuntimeError("no installed transaction / backup")
    stored = CACHE / "starvault-reverted-candidate-20261112"
    if stored.exists():
        raise RuntimeError("reverted candidate destination already exists")
    os.replace(LIVE, stored)
    try:
        os.replace(BACKUP, LIVE)
    except BaseException:
        os.replace(stored, LIVE)
        raise
    if sha(LIVE / OLD_PROVIDER) != record["old"]["provider"]:
        raise RuntimeError("rollback hash mismatch")
    record["status"] = "rolled_back"
    save(record)
    print("ROLLED_BACK", OLD_PROVIDER)

if __name__ == "__main__":
    try:
        action = sys.argv[1]
        if action == "prepare":
            prepare()
        elif action == "commit":
            commit()
        elif action == "rollback":
            rollback()
        elif action == "status":
            print(RECORD.read_text(encoding="utf-8") if RECORD.is_file() else "no transaction")
        else:
            raise RuntimeError("actions: prepare, commit, rollback, status")
    except BaseException as error:
        print("PUBLISH FAILED:", error, file=sys.stderr)
        raise SystemExit(1)
