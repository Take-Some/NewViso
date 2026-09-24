from __future__ import annotations

from collections import Counter, defaultdict
from hashlib import sha256
from pathlib import Path
import re
import tomllib

ROOT = Path(__file__).resolve().parents[1]

MAX_PRODUCTION_LINES = 600
DUPLICATE_WINDOW_LINES = 8
DUPLICATE_MIN_CHARS = 220

SOURCE_ROOTS = (ROOT / "crates", ROOT / "apps")
ARTIFACT_ROOTS = (ROOT / "crates", ROOT / "apps", ROOT / "scripts")

TRIVIAL_LINES = {
    "{",
    "}",
    "};",
    ");",
    "],",
}


def production_rust_files() -> list[Path]:
    files: list[Path] = []
    for source_root in SOURCE_ROOTS:
        if source_root.exists():
            files.extend(source_root.rglob("*.rs"))
    return sorted(path for path in files if ".bak-" not in path.name and not path.name.endswith(".rs.bk"))


def production_text(path: Path) -> str:
    text = path.read_text(encoding="utf-8", errors="ignore")
    # Test scaffolding is not runtime engine code and should not force runtime ownership splits.
    return text.split("#[cfg(test)]", 1)[0]


def normalized_significant_lines(text: str) -> list[tuple[int, str]]:
    result: list[tuple[int, str]] = []
    for line_number, line in enumerate(text.splitlines(), 1):
        normalized = re.sub(r"//.*$", "", line).strip()
        if not normalized or normalized in TRIVIAL_LINES:
            continue
        result.append((line_number, normalized))
    return result


def backup_artifacts() -> list[Path]:
    result: list[Path] = []
    for source_root in ARTIFACT_ROOTS:
        if not source_root.exists():
            continue
        for path in source_root.rglob("*"):
            if path.is_file() and (".bak-" in path.name or path.name.endswith(".rs.bk")):
                result.append(path)
    return sorted(result)


def duplicate_blocks(files: list[Path]) -> list[list[tuple[Path, int]]]:
    occurrences: dict[str, list[tuple[Path, int, str]]] = defaultdict(list)

    for path in files:
        lines = normalized_significant_lines(production_text(path))
        for index in range(len(lines) - DUPLICATE_WINDOW_LINES + 1):
            window = lines[index : index + DUPLICATE_WINDOW_LINES]
            body = "\n".join(line for _, line in window)
            if len(body) < DUPLICATE_MIN_CHARS:
                continue
            occurrences[sha256(body.encode("utf-8")).hexdigest()].append(
                (path, window[0][0], body)
            )

    groups: list[list[tuple[Path, int]]] = []
    seen_location_sets: set[tuple[tuple[str, int], ...]] = set()
    for matches in occurrences.values():
        locations = sorted({(path, line) for path, line, _ in matches}, key=lambda item: (str(item[0]), item[1]))
        if len(locations) < 2:
            continue
        key = tuple((str(path), line) for path, line in locations)
        if key in seen_location_sets:
            continue
        seen_location_sets.add(key)
        groups.append(locations)

    groups.sort(key=lambda group: [(str(path), line) for path, line in group])
    return groups



def workspace_manifest_errors() -> list[str]:
    manifest = ROOT / "Cargo.toml"
    try:
        data = tomllib.loads(manifest.read_text(encoding="utf-8"))
    except tomllib.TOMLDecodeError as error:
        return [f"Cargo.toml is invalid or contains a duplicate key: {error}"]

    members = data.get("workspace", {}).get("members", [])
    if not isinstance(members, list):
        return ["Cargo.toml workspace.members must be an array"]

    counts = Counter(str(member) for member in members)
    duplicates = sorted(member for member, count in counts.items() if count > 1)
    return [
        f"Cargo.toml workspace member is duplicated: {member}"
        for member in duplicates
    ]


def collect_errors() -> list[str]:
    errors: list[str] = []
    errors.extend(workspace_manifest_errors())
    files = production_rust_files()

    for path in files:
        line_count = len(production_text(path).splitlines())
        if line_count > MAX_PRODUCTION_LINES:
            errors.append(
                f"{path.relative_to(ROOT)}: production module has {line_count} lines; "
                f"limit is {MAX_PRODUCTION_LINES}. Split by ownership boundary."
            )

    for path in backup_artifacts():
        errors.append(
            f"{path.relative_to(ROOT)}: backup/source clone must not live in the source tree"
        )

    for locations in duplicate_blocks(files):
        rendered = ", ".join(
            f"{path.relative_to(ROOT)}:{line}" for path, line in locations[:6]
        )
        if len(locations) > 6:
            rendered += f", +{len(locations) - 6} more"
        errors.append(
            f"exact production clone detected across {DUPLICATE_WINDOW_LINES} significant lines: {rendered}"
        )

    return errors


def main() -> int:
    errors = collect_errors()
    if errors:
        print("NewViso code-health check: FAILED")
        for error in errors:
            print(f"  - {error}")
        return 1

    files = production_rust_files()
    largest = max(
        ((len(production_text(path).splitlines()), path) for path in files),
        default=(0, ROOT),
    )
    print(
        "NewViso code-health check: PASS "
        f"({len(files)} production Rust files, largest={largest[0]} lines "
        f"[{largest[1].relative_to(ROOT)}], exact clone groups=0, backup artifacts=0)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
