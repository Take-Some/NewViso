from __future__ import annotations

from pathlib import Path
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[1]
FORBIDDEN_MARKERS = (
    "NewEngineNorthStar",
    "NewEngineRockstar",
    "ModulesSrc",
    "Toolset-main",
)

DEPENDENCY_TABLES = (
    "dependencies",
    "dev-dependencies",
    "build-dependencies",
)


def inside_root(path: Path) -> bool:
    try:
        path.resolve().relative_to(ROOT)
        return True
    except ValueError:
        return False


def inspect_dependency_table(
    manifest: Path,
    table_name: str,
    table: dict[str, object],
    errors: list[str],
) -> None:
    for dependency_name, spec in table.items():
        if not isinstance(spec, dict):
            continue

        path_value = spec.get("path")
        if not isinstance(path_value, str):
            continue

        resolved = (manifest.parent / path_value).resolve()
        if not inside_root(resolved):
            errors.append(
                f"{manifest.relative_to(ROOT)}: {table_name}.{dependency_name} "
                f"escapes NewViso -> {resolved}"
            )


def inspect_manifest(manifest: Path, errors: list[str]) -> None:
    raw = manifest.read_text(encoding="utf-8")

    for marker in FORBIDDEN_MARKERS:
        if marker.casefold() in raw.casefold():
            errors.append(
                f"{manifest.relative_to(ROOT)} contains forbidden legacy marker: {marker}"
            )

    data = tomllib.loads(raw)

    for table_name in DEPENDENCY_TABLES:
        table = data.get(table_name)
        if isinstance(table, dict):
            inspect_dependency_table(manifest, table_name, table, errors)

    workspace = data.get("workspace")
    if isinstance(workspace, dict):
        table = workspace.get("dependencies")
        if isinstance(table, dict):
            inspect_dependency_table(
                manifest,
                "workspace.dependencies",
                table,
                errors,
            )


def main() -> int:
    errors: list[str] = []

    manifests = sorted(
        path
        for path in ROOT.rglob("Cargo.toml")
        if "target" not in path.parts
    )

    for manifest in manifests:
        inspect_manifest(manifest, errors)

    if errors:
        print("NewViso isolation check: FAILED")
        for error in errors:
            print(f"  - {error}")
        return 1

    print(
        f"NewViso isolation check: PASS "
        f"({len(manifests)} Cargo manifests, no external path dependencies)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
