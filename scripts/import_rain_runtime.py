"""Cook shared rain authoring policy into the shared GTA weather catalog."""
from pathlib import Path
import json
import shutil

def main():
    workspace = Path(__file__).resolve().parents[2]
    source = workspace / "Shared/Source/environment/gtav_weather/rain_runtime.json"
    target = workspace / "Shared/Content/environment/weather/gtav_weather.catalog.json"
    policy = json.loads(source.read_text(encoding="utf-8"))
    runtime = policy["runtime"]
    spacing = runtime["collision_cell_metres"]
    budget = runtime["collision_probes_per_frame"]
    if not isinstance(spacing, (int, float)) or not 0.5 <= spacing <= 8.0:
        raise ValueError("collision_cell_metres must be 0.5..8")
    if not isinstance(budget, int) or not 1 <= budget <= 1089:
        raise ValueError("collision_probes_per_frame must be 1..1089")
    catalog = json.loads(target.read_text(encoding="utf-8"))
    names = {row["name"] for row in catalog["weather_gpu_fx"]}
    counts = policy["particle_counts"]
    if not set(counts) <= names:
        raise ValueError("rain policy contains an unknown GPU layer")
    for count in counts.values():
        if not isinstance(count, int) or not 1 <= count <= 16384:
            raise ValueError("particle counts must be integers in 1..16384")
    catalog["rain"]["runtime"] = runtime
    for row in catalog["weather_gpu_fx"]:
        if row["name"] in counts:
            row["runtimeParticleCount"] = counts[row["name"]]
    result = json.dumps(catalog, indent=2, ensure_ascii=False) + "\n"
    if json.loads(target.read_text(encoding="utf-8")) != catalog:
        shutil.copy2(target, target.with_suffix(target.suffix + ".before-rain-cook"))
        target.write_text(result, encoding="utf-8", newline="\n")
    print(f"Shared rain policy cooked: {len(counts)} layers, {budget} probes/frame")

if __name__ == "__main__":
    main()
