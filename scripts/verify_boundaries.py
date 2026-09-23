from __future__ import annotations

from pathlib import Path
import tomllib

ROOT = Path(__file__).resolve().parents[1]

ALLOWED: dict[str, set[str]] = {
    "newviso": {"newviso-config", "newviso-host", "newviso-runtime"},
    "newviso-assets-client": {"newviso-host"},
    "newviso-audio-api": set(),
    "newviso-audio-client": {"newviso-audio-api", "newviso-host"},
    "newviso-capabilities": {"newviso-provider-runtime"},
    "newviso-compat-abi": set(),
    "newviso-config": set(),
    "newviso-content-manager": {"newviso-assets-client"},
    "newviso-core": set(),
    "newviso-host": {"newviso-compat-abi"},
    "newviso-input-client": {"newviso-host"},
    "newviso-platform": {"newviso-compat-abi", "newviso-host"},
    "newviso-physics-client": {"newviso-host"},
    "newviso-provider-runtime": {"newviso-compat-abi", "newviso-host"},
    "newviso-project": set(),
    "newviso-render-client": {"newviso-host"},
    "newviso-resource-runtime": {"newviso-assets-client"},
    "newviso-textures": {"newviso-resource-runtime"},
    "newviso-materials": {"newviso-resource-runtime", "newviso-textures"},
    "newviso-model": {"newviso-materials", "newviso-resource-runtime"},
    "newviso-runtime": {
        "newviso-compat-abi",
        "newviso-config",
        "newviso-content-manager",
        "newviso-core",
        "newviso-host",
        "newviso-input-client",
        "newviso-platform",
        "newviso-provider-runtime",
        "newviso-project",
        "newviso-scene",
        "newviso-materials",
        "newviso-model",
        "newviso-resource-runtime",
        "newviso-textures",
        "newviso-assets-client",
        "newviso-capabilities",
        "newviso-render-client",
        "newviso-scripting",
        "newviso-ui-client",
    },
    "newviso-scene": {
        "newviso-host",
        "newviso-render-client",
        "newviso-input-client",
        "newviso-materials",
        "newviso-model",
        "newviso-resource-runtime",
        "newviso-textures",
    },
    "newviso-script-client": {"newviso-host"},
    "newviso-scripting": {"newviso-assets-client", "newviso-script-client"},
    "newviso-ui-client": {"newviso-host"},
}

DEPENDENCY_TABLES = ("dependencies", "dev-dependencies", "build-dependencies")

FORBIDDEN_FORMAT_CRATES = {
    "newviso-audio-xvag",
    "newviso-nef8",
    "newviso-neui",
    "newviso-ydd",
    "newviso-ytd",
    "newviso-ymt",
}

FORBIDDEN_ENGINE_FORMAT_MARKERS = (
    "CodecRegistry",
    "newviso_nef8",
    "newviso_ydd",
    "newviso_ytd",
    "newviso_ymt",
    "newviso_audio_xvag",
    "NEF8",
    ".ydd",
    ".ytd",
    ".ymt",
    ".xvag",
    ".neui.xml",
)


def load_manifest(path: Path) -> dict:
    return tomllib.loads(path.read_text(encoding="utf-8"))

def internal_dependencies(data: dict) -> set[str]:
    result: set[str] = set()
    for table_name in DEPENDENCY_TABLES:
        table = data.get(table_name)
        if not isinstance(table, dict):
            continue
        result.update(name for name in table if name.startswith("newviso-"))
    return result

def manifests() -> list[Path]:
    paths = [ROOT / "apps" / "newviso" / "Cargo.toml"]
    paths.extend(sorted((ROOT / "crates").glob("newviso-*/Cargo.toml")))
    return [path for path in paths if path.is_file()]

def main() -> int:
    errors: list[str] = []
    workspace_text = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
    for forbidden in sorted(FORBIDDEN_FORMAT_CRATES):
        if forbidden in workspace_text:
            errors.append(f"workspace must not declare file-format crate {forbidden}")

    for source_root in (ROOT / "crates", ROOT / "apps", ROOT / "modules"):
        if not source_root.exists():
            continue
        for source in source_root.rglob("*"):
            if not source.is_file() or source.suffix.lower() not in {".rs", ".toml"}:
                continue
            text = source.read_text(encoding="utf-8", errors="ignore")
            for marker in FORBIDDEN_ENGINE_FORMAT_MARKERS:
                if marker.lower() in text.lower():
                    errors.append(
                        f"{source.relative_to(ROOT)}: concrete asset format marker is forbidden in engine source: {marker}"
                    )
    seen: set[str] = set()
    for manifest in manifests():
        data = load_manifest(manifest)
        package = data.get("package", {})
        name = package.get("name")
        if not isinstance(name, str):
            errors.append(f"{manifest.relative_to(ROOT)}: missing package.name")
            continue
        seen.add(name)
        if name not in ALLOWED:
            errors.append(f"{name}: no architecture boundary rule declared")
            continue
        deps = internal_dependencies(data)
        forbidden = sorted(deps - ALLOWED[name])
        if forbidden:
            errors.append(f"{name}: forbidden internal dependencies: {', '.join(forbidden)}")
    missing_rules = sorted(set(ALLOWED) - seen)
    if missing_rules:
        errors.append("architecture rules reference missing packages: " + ", ".join(missing_rules))
    if errors:
        print("NewViso architecture boundary check: FAILED")
        for error in errors:
            print(f"  - {error}")
        return 1
    print(f"NewViso architecture boundary check: PASS ({len(seen)} packages, dependency directions are valid)")
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
