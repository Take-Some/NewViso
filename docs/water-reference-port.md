# Water reference port

The shared water runtime uses one water clock and one height field for rendering, buoyancy and gameplay queries. The old surface was animated with the accelerated sky clock while physics and scripts evaluated separate sine waves. Advancing the sky or resetting its phase could move the visible surface without moving the physical water.

## Reference and implementation

Reference root: `C:\Games\gta v source code\GTAV Source\src\dev_ng\game`.

| Reference | NewViso implementation |
| --- | --- |
| `renderer/waterdefines.h`, `WaterUpdateDynamicCommon.h` | `crates/newviso-scene/src/water.rs`: PC 256 x 256 grid, 2 m cells, fixed integration, source L8 forcing sampler, spring/drag, wave and calming quads |
| `shader_source/Water/waterTex.fx`, `UpdateBumpCommon` | `assets/water_bump.frag`, `renderer_water_bump.rs`: 256 x 256 ping-pong height/velocity and derived normals |
| `water_common.fxh`, `GetOceanBump`, `WaterCommon` | `assets/water.frag`: dynamic detail, 3.7 octave, far-cell noise masking, low bump channels, rain normals, extinction, depth guard and foam |
| `RenderPhaseWaterReflection.cpp` | `renderer_water.rs`: reflected camera, plane clip and separate reflection visibility; opaque and alpha-tested materials plus sorted alpha ranges |
| `waterTex.fx`, `PS_WaterUpdateWet`, `water_terrainfoam.fx` | `water.rs`, `water_shore.frag`: persistent crest tension and wet-height history; foam on actual shore geometry |
| `waterTex.fx`, fog composite; `water.fx`, underwater branch | `renderer_water_underwater.rs`, `water_underwater.frag`, `water.frag`: whole HDR water fog, dual animated source caustics, immersion blend and underside refraction/TIR |
| Original `OCEAN_BEACH_SOUNDSET`, AWC/REL data | `scripts/import_water_audio.py`, Shared `water_audio.ysc`: original PCM dependencies, persistent loop voices, shore-wave cues and underwater mix |
| Original `core.ypt` water rules | `scripts/import_water_effects.py`: native cooked YPT, texture dictionary and model geometry; Shared gameplay triggers contacts and movement |

The large-wave solver uses axis weights .17, diagonal weights .08, spring 40, equilibrium .5, exponential friction .7 and the .75 timestep multiplier. The source DDS sampler uses width minus one, source fixed byte conversion and the original forcing offset. Small GPU ripples use the separate source .18/.07 stencil, restoring force .18 and normal reconstruction. Water time is independent of the accelerated climate clock.

Physics advances the shared simulation during fixed steps. The vertex shader samples the same uploaded height/velocity/foam field. Whole-cell focus shifts retain waves at their existing world positions. Impacts change velocity rather than directly changing height. A long frame has a bounded integration budget.

Near water is a static authored 256 x 256 mesh. Its vertex shader recentres it over the moving field. Far water is excluded from that footprint so a large flat triangle cannot erase animated crests. Water is excluded from the opaque GBuffer and shadow geometry and is drawn in its own pass.

Reflection, refraction colour and linear scene-depth targets use half the viewport resolution. Reflection keeps the main shadow map's light/focus contract while using the mirrored viewing camera. Depth lookup uses point sampling and silhouette rejection; colour lookup uses linear sampling. Bindings fit the render provider's four descriptor-set limit. Frame-owned buffer rings prevent writes into uniforms still in flight. The water surface writes main scene depth, so alpha geometry across the surface is visible through the optical targets rather than overwriting the resolved water. Source particles are included in the colour targets. The original R32 opaque geometry distance remains separate for underwater caustics; caustics are masked when the surface occludes that geometry. Underwater HDR extent is selected from the scene HDR resource, independently of shadow-target ordering.

Custom water-fog commands retain their explicit render phase when selecting a transparent draw list. Backend graph-texture descriptors are resolved before command replay for both ordinary sampled textures and sampled depth attachments. Completed depth targets use the shader-read layout; depth sampled while still attached uses the combined read-only depth layout. These contracts apply to all projects.

## Content and authoring

Original DDS files are retained byte-for-byte under `Shared/Source/environment/water`; runtime DDS files are under `Shared/Content/textures/environment/water`. SHA-256 checks are recorded in `Shared/Source/environment/water/import-manifest.json`.

| Original DDS | Runtime name |
| --- | --- |
| waterbump.dds | normal_a.dds |
| waterbump2.dds | normal_b.dds |
| waterfoam.dds | foam.dds |
| waterfog.dds | fog.dds |
| waternoise.dds | noise.dds |
| oceanwaves.dds | ocean_forcing.dds |
| shorewaves.dds | shore_forcing.dds |
| caustic.dds | caustic.dds |

The imported water-effect closure contains 127 effects, 237 emitters, 192 particle rules, 105 textures and 3 model geometries, with zero unresolved texture references. The manifest records source/runtime hashes and the complete behaviour inventory. The two runtime assets are `particles/environment/water/reference.ypt` and `textures/environment/water/effects.ytd`.

`Shared/Content/scripts/newviso/world/water.ysc` receives runtime water samples. `water_effects.ysc` selects native ped, vehicle and object entry/exit/trail rules, with contact hysteresis and rate limits. Fully submerged bodies remain wet; peds emit bubbles and underwater trails, then drips after leaving water. The Shared default gameplay module owns these effects; Island's old effect helper no longer produces duplicates. Exact entity keys are retained as strings in script snapshots.

Author water regions in the root `water` object of the scene, before ECS normalisation:

```json
{
  "water": {
    "wave_quads": [
      {"min": [0, 0], "max": [12, 12], "direction": [0, 1], "amplitude": 1}
    ],
    "calming_quads": [
      {"min": [20, 20], "max": [40, 40], "damping": 0.2}
    ]
  }
}
```

Coordinates are world X/Z metres. The engine contains no Island shoreline equation. Island's authoring tool creates 24 wave regions from its actual terrain zero crossings. Its near-water mesh has 256 rows/columns and a 510 m span. The root regions are retained before the scene provider rewrites ECS records.

The validated common optical profile lives at `Shared/Content/environment/water/defaults.json`, with its authoring copy under Shared/Source. Individual scene `water.optics` overrides retain other common defaults. Caustics use the original elapsed-seconds / 40 clock, independently of accelerated climate time.

`Shared/Content/audio/environment/water/water.yscd` contains four cues (ocean, wave_break, wave_end, underwater) from ten original recordings. Nine PCM wave hashes are resolved from the original REL sound-set dependency graph; the original eight-channel underwater stream is downmixed to stereo at 48 kHz. Source banks, decoded WAVs, hashes and dependency inventory are retained in `Shared/Source/audio/environment/water`. `water_audio.ysc` varies persistent loop voice gains without restarting playback, places shore sounds using authored wave regions, and crossfades on immersion. The engine's generic `audio.loop.set` command owns and cleans up loop voices.

Physical density and drag are configured through `physics.water.configure`. `Projects/Island/water.json` documents the project defaults; it is not a second independently advancing solver.

## Verification

The continuation compiled 34 shaders with bundled glslang. Twelve scene water/reflected-camera tests and one buoyancy test passed. One underwater graph regression verifies the HDR viewport extent and the retained original geometry depth. Ten Vulkan provider tests verify command routing, sampled graph-texture refresh and attached/completed depth layouts.

The numerical tests cover frame partitioning, retaining waves during focus shifts, non-teleporting impacts, invalid deltas and hitches, authored shore/calming regions, normals, submersion, validated optics overrides and persistent foam/wet history. Camera immersion has hysteresis and continuous optical blending. The physics test verifies buoyancy impulses without teleport or angular-velocity overrides.

| Native run | Frames | Exit | Logged errors |
| --- | ---: | ---: | ---: |
| Island-content water fixture | 3600 | 0 | 0 |
| Real Island gameplay entry | 900 | 0 | 0 |
| Scene without water | 180 | 0 | 0 |
| Installed executable water smoke | 180 | 0 | 0 |

The fixture is under `Tools/tmp/water-work/IslandSmoke`, separate from Island's gameplay entry. It uses source splash rules, gravity 18 at 120 Hz, a low-density floating crate and four camera stages: above water, submerged near the floor, looking up through the surface, and exiting. Shader validation is strict. Captures come from the actual engine window.

All eight runtime DDS SHA-256 hashes match their original files and Shared import manifest. The four-cue audio dictionary matches its ten-recording source manifest. Ocean loop voice 1 persists through all camera stages. Underwater voice 7 persists through both submerged stages and is released after exit.

Native captures confirm wave motion, shore foam, reflected/refracted geometry, source splash particles, bubbles, caustics and immersion. Recording water depth removed the rectangular alpha/cloud artifacts and direct above-surface palm overlays in the underwater view. The source refraction/reflection targets were inspected separately.

A remaining near-floor triangular lighting band was reproduced with the water level lowered to -100 m, immersion blend zero and the underwater graph pass absent. It is a separate base terrain/rendering issue rather than a water-fog or particle artifact.

Internal diagnostic environment variable `NEWVISO_WATER_DIAG_OPTICS` accepts `reflection`, `refraction`, `depth` or `scene`. These modes show the optical targets or ordinary scene colour with fog weight zero. The variable is unset for all final normal water runs.

The updated executable is installed at `NewViso/target/release/newviso.exe`; the matching Vulkan provider is under `pluginsRuntime`. Backups, native logs/captures and `final-water-validation.json` are under `Tools/tmp/water-work`.

## Fidelity boundary

This is a reference-derived port into NewViso's renderer, not the original RAGE renderer. Source textures, particle data and the imported PCM recordings are preserved. The continuation ports water-fog/caustic formulas into NewViso's HDR graph; its lighting inputs and surface-depth approximation still differ from the complete original prepass/blur/light buffers. River/fountain branches, dual-paraboloid reflection fallback, per-bone/per-hull boat wake placement, dedicated CloudHat reflection rendering, ModularSynthSound DSP and the original dynamic audio mix graph remain unported. Main-scene lighting, colour output and the source particle behaviour evaluator can also differ. Matching data counts or a successful launch does not prove pixel-identical output.

The Island shore rectangles belong to its authored terrain; they are not the GTA map's water-quads file. A matched-camera comparison against an original running scene is required before claiming complete visual parity.

## Vegetation and first-person follow-up

The later material/reflection correction rebuilds 35 packaged shaders, retains source vegetation color/shadow pass flags, makes heightfield sampler selection explicit, and excludes camera-local first-person geometry from SSR and all water optical targets while retaining world geometry. The follow-up installed native runs cover 3600 fixture frames, 900 actual Island frames and 180 no-water frames with zero logged errors. See vegetation-first-person-reflections.md and Tools/tmp/water-work/final-vegetation-reflection-validation.json for the complete scope and evidence.
