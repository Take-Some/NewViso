"""Rebuild the packaged scene SPIR-V after changing scene.vert/scene.frag.
Requires the Vulkan SDK on PATH. Normal cargo builds use the checked-in binaries.
"""
from pathlib import Path
import shutil
import subprocess

root = Path(__file__).resolve().parents[1]
assets = root / "crates/newviso-scene/src/assets"
compiler = shutil.which("glslc")
validator = shutil.which("spirv-val")
if not compiler or not validator:
    raise SystemExit("Install the Vulkan SDK and add its Bin directory to PATH.")
for stem in ("scene", "sky", "shadow", "flare"):
    for stage in ("vert", "frag"):
        source = assets / f"{stem}.{stage}"
        output = source.with_suffix(source.suffix + ".spv")
        subprocess.run([compiler, "--target-env=vulkan1.0", "-O", str(source), "-o", str(output)], check=True)
        subprocess.run([validator, "--target-env", "vulkan1.0", str(output)], check=True)
        print(f"{output.name}: {output.stat().st_size} bytes, validated")
