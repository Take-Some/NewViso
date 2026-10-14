"""Rebuild packaged scene SPIR-V after changing scene shaders.

Prefers Vulkan SDK glslc + spirv-val when available. NewViso also ships against
VulkanRenderer's pinned glslangValidator, which is a valid offline fallback for
developer machines without a separately installed Vulkan SDK.
"""
from pathlib import Path
import shutil
import subprocess

root = Path(__file__).resolve().parents[1]
northstar_root = root.parent
assets = root / "crates/newviso-scene/src/assets"
shared_shader_includes = root / "crates/newviso-render-api/shaders"

glslc = shutil.which("glslc")
spirv_val = shutil.which("spirv-val")
bundled_glslang = (
    northstar_root
    / "PluginsSrc"
    / "VulkanRenderer"
    / "newengine-modules-render-vulkan-ash"
    / "tools"
    / "glslang"
    / "bin"
    / "glslangValidator.exe"
)

if glslc and spirv_val:
    backend = "vulkan-sdk"
elif bundled_glslang.is_file():
    backend = "bundled-glslang"
else:
    raise SystemExit(
        "No GLSL compiler found. Install Vulkan SDK or restore VulkanRenderer/tools/glslang."
    )

shader_sources = [
    *( (stem, stage)
       for stem in (
           "scene",
           "sky",
           "shadow",
           "flare",
           "atmospheric_cloud",
           "atmospheric_depth",
           "atmospheric_depth_instanced",
           "volumetric_cloud",
           "volumetric_cloud_temporal",
           "volumetric_cloud_composite",
           "weather_world",
           "weather_lens",
       )
       for stage in ("vert", "frag") ),
    ("skinning", "comp"),
    ("scene_gbuffer", "frag"),
    ("scene_instanced", "vert"),
    ("shadow_instanced", "vert"),
    ("water", "vert"),
    ("water", "frag"),
    ("water_bump", "vert"),
    ("water_bump", "frag"),
    ("water_depth", "frag"),
    ("water_shore", "frag"),
    ("water_underwater", "vert"),
    ("water_underwater", "frag"),
]

for stem, stage in shader_sources:
    source = assets / f"{stem}.{stage}"
    output = source.with_suffix(source.suffix + ".spv")

    if backend == "vulkan-sdk":
        subprocess.run(
            [glslc, "--target-env=vulkan1.0", "-O", f"-I{assets}", f"-I{shared_shader_includes}", str(source), "-o", str(output)],
            check=True,
        )
        subprocess.run(
            [spirv_val, "--target-env", "vulkan1.0", str(output)],
            check=True,
        )
    else:
        subprocess.run(
            [
                str(bundled_glslang),
                "--target-env",
                "vulkan1.0",
                f"-I{assets}",
                f"-I{shared_shader_includes}",
                str(source),
                "-o",
                str(output),
            ],
            check=True,
        )

    print(
        f"{output.name}: {output.stat().st_size} bytes, "
        f"compiled backend={backend}"
    )
