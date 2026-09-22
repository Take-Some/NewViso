from __future__ import annotations

from pathlib import Path
import tomllib

ROOT = Path(__file__).resolve().parents[1]

ALLOWED: dict[str, set[str]] = {
    "newviso": {"newviso-config", "newviso-runtime"},
    "newviso-compat-abi": set(),
    "newviso-config": set(),
    "newviso-core": set(),
    "newviso-host": {"newviso-compat-abi"},
    "newviso-platform": {"newviso-compat-abi", "newviso-host"},
    "newviso-provider-runtime": {"newviso-compat-abi", "newviso-host"},
    "newviso-render-client": {"newviso-host"},
    "newviso-runtime": {
        "newviso-compat-abi",
        "newviso-config",
        "newviso-core",
        "newviso-host",
        "newviso-platform",
        "newviso-provider-runtime",
        "newviso-scene",
    },
    "newviso-scene": {"newviso-host", "newviso-render-client"},
}

DEPENDENCY_TABLES = ("dependencies", "dev-dependencies", "build-dependencies")

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
