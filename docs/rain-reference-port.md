# Shared precipitation reference adaptation

Source reference: C:/Games/gta v source code/GTAV Source.

## Examined implementation

- rage/suite/src/gpuptfx/ptxgpudrop.cpp: emitter bounds flatten camera right/forward onto the horizontal plane, use world up, and add negative camera velocity to directional streaks (lines 447вЂ“488).
- game/shader_source/VFX/ptxgpu_update.fx: world gravity, wind drag toward air velocity, wrapped emitter volume and height-map collision (lines 327вЂ“344, 391вЂ“429, 488вЂ“574).
- game/shader_source/VFX/ptxgpu_render.fx: velocity-aligned drops, world-up billboards, authored sheet animation and second-frame range (lines 215вЂ“262, 389вЂ“405).
- game/vfx/gpuparticles/PtFxGPUSystems.cpp: resident precipitation collision height target (lines 117вЂ“174).

## Ownership and activation

Implementation belongs to NewViso/newviso-scene and newviso-runtime, not a project script. Every project that mounts Shared/Content and enables weather GPU FX receives it.

Existing shared assets remain the source of visual content:
environment/weather/gtav_weather.catalog.json,
textures/environment/weather/fxweather.ytd,
textures/environment/weather/weather_surface.ytd.

Editable density and collision tuning:
Shared/Source/environment/gtav_weather/rain_runtime.json.
Cook after authoring or reimporting weather:
python NewViso/scripts/import_rain_runtime.py

RAIN and THUNDER resolve their DROP, MIST and GROUND layers from the shared catalog. Existing climate cycles and scene.weather.state.set select weather; projects are not forced into permanent rain.

## Behavior

- Gravity is always world Y-down, independent of camera pitch/roll.
- Wind is a velocity response, rather than a camera-space displacement.
- Drop streaks use their current velocity with authored camera-speed compensation.
- Random particle seeds are anchored in world space; particles wrap inside the moving emitter volume.
- Sprite dimensions share a size seed. Unanimated sheets use a random frame; animated sheets use authored rate and lifetime scaling. Filtering is restricted to the selected atlas tile.
- Ground FX render an upright source splash and a separate horizontal expanding ring from the authored second sheet range.
- A completed 33x33 height map supplies individual particle surface heights. Resident triangle BVHs, scene heightfields/primitives and enabled water contribute. Trigger volumes and actor bodies are excluded.
- World rain remains visible outside while the camera is sheltered. Lens exposure remains controlled separately by the existing optics/underwater policy.
- Density controls draw count without a second multiplication of rain strength through alpha.

## Performance and fidelity limits

Default map spacing is 4 metres, covering 128x128 metres; 99 samples per frame complete a generation in 11 frames. GPU uniform size is 4752 bytes per layer, below Vulkan's 16 KiB minimum uniform limit. Completed generations are swapped together and camera teleports invalidate the old map.

This is an adaptation to NewViso's current renderer, not binary parity with RAGE. The original maintains persistent particle state and a GPU-rendered collision height target. NewViso evaluates analytic gravity/drag trajectories and uses a budgeted CPU-sampled field of resident geometry. Small roof edges can be approximated at the configured cell spacing; collider residency and query depth (512 metres, starting 256 metres above the camera) bound the map. Moving actor/vehicle roofs are not included. Full scene-color refraction and per-droplet local-light shading are not implemented in the world precipitation pass; existing wet-surface and lens rendering remain separate.

## Validation on 2026-10-11

- Both weather_world GLSL shaders compiled with the bundled glslangValidator for Vulkan 1.0.
- cargo test --offline -p newviso-scene -p newviso-runtime weather: 10 passed.
- cargo build --offline --release -p newviso -j1: passed.
- Disposable rendered climate test: RAIN, SNOW, THUNDER and checkpoint restoration passed; 429 timecycle channels restored.
- Live pre-shutdown state confirmed shared sources, GPU resources and the completed collision map, with all three THUNDER world layers active and wetness above zero. The climate verifier now asserts these precipitation conditions.
