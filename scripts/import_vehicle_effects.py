"""Import the original base vehicle PTFX graph and its textures into Shared Content."""
from pathlib import Path
import argparse
import hashlib
import json
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / "Shared/Source/vehicles/effects"
INTERMEDIATE = ROOT / "Shared/Intermediate/vehicles/effects"
NORMALIZER = ROOT / "Shared/Source/environment/gtav_weather/tools/ypt_normalizer/target/release/northstar-weather-ypt-normalizer.exe"
COOKER = ROOT / "Projects/VehicleTest/tools/native_rsc7_cooker/bin/north-yankton-native-rsc7-cooker.exe"
YTD_PACKER = ROOT / "Tools/toolbelt/first_party/northstar/ytd_packer/bin/northstar-ytd-packer.exe"
LOGICAL = "particles/vehicles/reference.ypt"
TEXTURES = "textures/vehicles/effects.ytd"


def run(args):
    result = subprocess.run(list(map(str, args)), capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=180)
    if result.returncode:
        raise RuntimeError(result.stdout + result.stderr)
    return result


def selected(name):
    return name.startswith(("veh_", "wheel_", "fire_wrecked_", "fire_petroltank_", "glass_", "exp_grd_vehicle")) or name in {
        "fire_vehicle", "fire_wheel", "fire_wheel_bike", "fire_petrol_pool", "fire_petrol_one", "fire_petrol_half", "fire_object",
        "bang_carmetal", "bul_carmetal", "bul_carmetal_heli", "mel_carmetal", "scrape_metal", "scrape_plastic", "scrape_concrete",
        "ent_brk_metal_frag", "ent_dst_metal_frag", "bul_glass", "bul_glass_heli", "bul_glass_mini", "bul_glass_shotgun",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--core", type=Path, default=ROOT / "Shared/Source/environment/gtav_weather/original/core.ypt")
    args = parser.parse_args()
    original = SOURCE / "original/core.ypt"
    original.parent.mkdir(parents=True, exist_ok=True)
    if original.resolve() != args.core.resolve():
        shutil.copy2(args.core, original)
    INTERMEDIATE.mkdir(parents=True, exist_ok=True)
    full_path = INTERMEDIATE / "core.full.json"
    texture_path = ROOT / "Shared/Content" / TEXTURES
    run([NORMALIZER, original, full_path])
    full = json.loads(full_path.read_text(encoding="utf-8"))
    names = [effect["name"] for effect in full["effects"] if selected(effect["name"])]
    model_path = INTERMEDIATE / "vehicle.source.json"
    raw_texture = INTERMEDIATE / "vehicle-ptfx-textures.ytd"
    run([NORMALIZER, original, model_path, "--models", "--textures-ytd", raw_texture, *names])
    full = json.loads(model_path.read_text(encoding="utf-8"))
    jobs = INTERMEDIATE / "texture.jobs.json"
    results = INTERMEDIATE / "texture.results.json"
    jobs.write_text(json.dumps([{"input": str(raw_texture), "output": str(raw_texture), "logical": TEXTURES}]), encoding="utf-8")
    run([COOKER, "cook", jobs, results])
    texture_dir = Path(json.loads(results.read_text(encoding="utf-8"))[0]["extract_dir"])
    run([YTD_PACKER, "pack", "--input-dir", texture_dir, "--output", texture_path])
    run([YTD_PACKER, "validate", "--input", texture_path])
    assert texture_path.read_bytes()[:4] == b"NEF8", "runtime textures must use the native container"
    effects = [effect for effect in full["effects"] if selected(effect["name"])]
    emitter_names = {event["emitter_rule"] for effect in effects for event in effect["events"] if event.get("emitter_rule")}
    particle_names = {event["particle_rule"] for effect in effects for event in effect["events"] if event.get("particle_rule")}
    emitters = [emitter for emitter in full["emitters"] if emitter["name"] in emitter_names]
    particles = [particle for particle in full["particles"] if particle["name"] in particle_names]
    assert emitter_names == {emitter["name"] for emitter in emitters}, "missing original emitter"
    assert particle_names == {particle["name"] for particle in particles}, "missing original particle"

    clips_path = SOURCE / "original/data/effects/ptxclipregions.dat"
    if not clips_path.exists():
        run([ROOT / "Tools/rsc7-cli/bin/rsc7.exe", "--no-update-check", "--keys", ROOT / "Tools/rsc7-cli/keys", "extract",
             r"C:\Games\Grand Theft Auto V\common.rpf", "data/effects/ptxclipregions.dat", "-o", SOURCE / "original", "--recursive"])
    clips = {}
    for line in clips_path.read_text(encoding="utf-8").splitlines():
        values = line.split()
        if len(values) >= 3 and not values[0].startswith("#"):
            try:
                clips[values[0].lower()] = [int(values[2]), int(values[1])]
            except ValueError:
                pass
    textures = {texture["name"].lower() for texture in full["textures"]}
    unresolved = []
    modes = {}
    for particle in particles:
        for model in particle.get("model_drawables", []):
            texture = model.get("texture_name")
            if texture:
                if texture.lower() not in textures:
                    unresolved.append({"particle": particle["name"], "model": model["name"], "texture": texture})
                else:
                    model["texture_ref"] = TEXTURES + "@" + texture
        diffuse = next((var.get("texture_name", "") for var in particle.get("shader_vars", [])
                        if var.get("type") == "texture" and var.get("name_hash") == "0x0df47048"), "")
        if diffuse:
            if diffuse.lower() not in textures:
                unresolved.append({"particle": particle["name"], "texture": diffuse})
            else:
                particle["texture_ref"] = TEXTURES + "@" + diffuse
                particle["texture_grid"] = clips.get(diffuse.lower(), [1, 1])
        for behaviour in particle.get("behaviours", []):
            modes[behaviour["type"]] = modes.get(behaviour["type"], 0) + 1
    if unresolved:
        raise RuntimeError("unresolved source particle textures: " + json.dumps(unresolved))
    runtime = {**full, "effects": effects, "emitters": emitters, "particles": particles,
               "texture_dictionary": TEXTURES, "source": LOGICAL, "runtime_container": "NEF16",
               "counts": {"effects": len(effects), "emitters": len(emitters), "particles": len(particles), "textures": len(textures)}}
    enriched = INTERMEDIATE / "vehicle.runtime.json"
    enriched.write_text(json.dumps(runtime, separators=(",", ":")) + "\n", encoding="utf-8")
    output = ROOT / "Shared/Content" / LOGICAL
    run([NORMALIZER, enriched, INTERMEDIATE / "vehicle.packed.json", "--runtime-json", "--nef16", output, "--logical-path", LOGICAL])
    assert output.read_bytes()[0x50:0x58] == b"NEF16\0\0\0", "expected native NEF16"
    report = {"schema": "newviso.vehicle.effects.import.v1", "counts": runtime["counts"], "source_sha256": hashlib.sha256(original.read_bytes()).hexdigest(),
              "runtime_sha256": hashlib.sha256(output.read_bytes()).hexdigest(), "texture_sha256": hashlib.sha256(texture_path.read_bytes()).hexdigest(),
              "logical_path": LOGICAL, "texture_dictionary": TEXTURES, "effects": [effect["name"] for effect in effects], "behaviour_inventory": modes,
              "unresolved_textures": unresolved}
    report["model_geometry_count"] = full.get("model_geometry_count", 0)
    report["model_particle_rules"] = sum(bool(p.get("model_drawables")) for p in particles)
    (SOURCE / "import-manifest.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"passed": True, "counts": runtime["counts"], "path": LOGICAL, "unresolved_textures": len(unresolved)}))


if __name__ == "__main__":
    main()
