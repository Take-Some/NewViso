#!/usr/bin/env python3
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tomllib


def run(command: list[str], cwd: Path) -> None:
    print(f"[rage-assets] cwd={cwd}")
    print("[rage-assets] exec=" + " ".join(command))
    subprocess.run(command, cwd=cwd, check=True)


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def package_version(cargo_toml: Path) -> str:
    data = tomllib.loads(cargo_toml.read_text(encoding="utf-8"))
    value = str(data.get("package", {}).get("version", "")).strip()
    if not value:
        raise RuntimeError(f"missing package.version: {cargo_toml}")
    return value


def retire_other_asset_manager_dlls(runtime_providers: Path, keep: Path) -> None:
    retired = runtime_providers / "_retired_asset_manager"
    for candidate in sorted(runtime_providers.glob("starVault-assetManager-*.dll")):
        if candidate.resolve() == keep.resolve():
            continue
        retired.mkdir(parents=True, exist_ok=True)
        destination = retired / candidate.name
        if destination.exists():
            destination.unlink()
        candidate.replace(destination)
        print(f"[rage-assets] retired provider={candidate.name}")


def update_runtime_manifest(manifest_path: Path, codec_path: Path) -> None:
    if manifest_path.is_file():
        document = json.loads(manifest_path.read_text(encoding="utf-8"))
    else:
        document = {
            "schema": "newengine.asset_manager.codec_manifest.v1",
            "profile": "release",
            "codecs": [],
        }

    codecs = document.setdefault("codecs", [])
    file_name = codec_path.name
    entry = {
        "file": file_name,
        "bytes": codec_path.stat().st_size,
        "sha256": sha256_file(codec_path),
    }

    replaced = False
    for index, current in enumerate(codecs):
        current_name = current.get("file") or current.get("dll") or current.get("path")
        if current_name == file_name or str(current_name).startswith("newengine-codec-rage-"):
            codecs[index] = entry
            replaced = True
            break
    if not replaced:
        codecs.append(entry)

    codecs.sort(key=lambda item: str(item.get("file") or item.get("dll") or item.get("path") or ""))
    manifest_path.write_text(
        json.dumps(document, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Build, verify and deploy native RSC7 YDR/YBN support into NewViso"
    )
    parser.add_argument(
        "--skip-tests",
        action="store_true",
        help="Skip cargo tests; intended only for packaging an already verified tree.",
    )
    parser.add_argument(
        "--debug",
        action="store_true",
        help="Build debug/dev artifacts instead of release deployment.",
    )
    args = parser.parse_args()

    newviso = Path(__file__).resolve().parents[1]
    northstar = newviso.parent
    asset_manager = northstar / "PluginsSrc" / "AssetManager"
    formats_workspace = northstar / "PluginsSrc" / "formats"
    rage_crate = asset_manager / "codecs" / "newengine-codec-rage"
    asset_manager_crate = asset_manager / "newengine-AssetManager"
    runtime_providers = newviso / "runtime" / "providers"
    runtime_codecs = runtime_providers / "codecs"
    runtime_formats = runtime_providers / "formats"

    for required in [
        asset_manager,
        asset_manager_crate,
        formats_workspace,
        rage_crate,
        runtime_providers,
    ]:
        if not required.exists():
            raise RuntimeError(f"required path does not exist: {required}")

    runtime_codecs.mkdir(parents=True, exist_ok=True)
    runtime_formats.mkdir(parents=True, exist_ok=True)

    cargo_profile = "dev" if args.debug else "release"
    target_profile = "debug" if args.debug else "release"
    suffix = "dev" if args.debug else "release"

    if not args.skip_tests:
        run(
            ["cargo", "test", "-p", "newengine-codec-rage", "--profile", cargo_profile],
            asset_manager,
        )
        run(
            [
                "cargo",
                "test",
                "-p",
                "newviso-resource-runtime",
                "-p",
                "newviso-semantic-assets",
            ],
            newviso,
        )

    run(
        [
            "cargo",
            "build",
            "-p",
            "engine-assets-starvault",
            "-p",
            "newengine-codec-rage",
            "--profile",
            cargo_profile,
        ],
        asset_manager,
    )

    asset_manager_version = package_version(asset_manager_crate / "Cargo.toml")
    source_asset_manager = asset_manager / "target" / target_profile / "asset_manager.dll"
    if not source_asset_manager.is_file():
        raise FileNotFoundError(
            f"missing AssetManager build artifact: {source_asset_manager}"
        )
    deployed_asset_manager = (
        runtime_providers
        / f"starVault-assetManager-{asset_manager_version}-{suffix}.dll"
    )
    shutil.copy2(source_asset_manager, deployed_asset_manager)
    retire_other_asset_manager_dlls(runtime_providers, deployed_asset_manager)
    print(
        f"[rage-assets] installed provider={deployed_asset_manager.name} "
        f"bytes={deployed_asset_manager.stat().st_size}"
    )

    version = package_version(rage_crate / "Cargo.toml")
    source_codec = asset_manager / "target" / target_profile / "newengine_codec_rage.dll"
    if not source_codec.is_file():
        raise FileNotFoundError(f"missing codec build artifact: {source_codec}")

    deployed_codec = runtime_codecs / f"newengine-codec-rage-{version}-{suffix}.dll"
    shutil.copy2(source_codec, deployed_codec)
    print(
        f"[rage-assets] installed codec={deployed_codec.name} "
        f"bytes={deployed_codec.stat().st_size}"
    )

    format_builder = formats_workspace / "build_formats.py"
    command = [
        sys.executable,
        str(format_builder),
        "--profile",
        "debug" if args.debug else "release",
        "--format",
        "ydr",
        "--format",
        "ybn",
        "--install-dir",
        str(runtime_formats),
    ]
    run(command, formats_workspace)

    for extension in ("ydr", "ybn"):
        module = runtime_formats / f"{extension}.dll"
        if not module.is_file():
            raise FileNotFoundError(f"missing format module after build: {module}")
        print(
            f"[rage-assets] installed format={module.name} "
            f"bytes={module.stat().st_size}"
        )

    if not args.debug:
        update_runtime_manifest(runtime_codecs / "codec_manifest.json", deployed_codec)

    print(
        f"[rage-assets] StarVault={asset_manager_version} "
        "owns RSC7 recognition and codec dispatch"
    )
    print("[rage-assets] YDR -> model.runtime_v1")
    print("[rage-assets] YBN -> collision.runtime_v1")
    print("[rage-assets] RSC7 magic -> AssetManager codec worker")
    print("[rage-assets] deployment complete")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
