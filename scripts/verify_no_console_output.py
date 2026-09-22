from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]
SOURCE_ROOTS = [ROOT / "apps", ROOT / "crates"]
FORBIDDEN = re.compile(r"\b(?:print|println|eprint|eprintln|dbg)!\s*\(")

errors = []
for source_root in SOURCE_ROOTS:
    if not source_root.exists():
        continue
    for path in source_root.rglob("*.rs"):
        text = path.read_text(encoding="utf-8")
        for line_number, line in enumerate(text.splitlines(), 1):
            if FORBIDDEN.search(line):
                errors.append(f"{path.relative_to(ROOT)}:{line_number}: {line.strip()}")

if errors:
    print("NewViso direct console output check: FAILED")
    for error in errors:
        print(f"  - {error}")
    raise SystemExit(1)

print("NewViso direct console output check: PASS")
