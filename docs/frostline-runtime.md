# Snow, cold and Frostline

Implemented 2026-10-12. This is an executable foundation and a new winter project,
not a verified reproduction of every Frostpunk system or its original balance.

## Ownership and extension

- `newviso-environment/src/snow.rs`: validated snow grid, exact triangle sampling,
  compaction, fresh snow, melting, checkpoint data and dirty vertices.
- `newviso-environment/src/thermal.rs`: generic thermal bodies and non-stacking heat zones.
- `newviso-runtime/src/application_cold.rs`: script commands, body bindings,
  actual physics motion, thermal integration and coalesced terrain/collider updates.
- `newviso-scene/src/first_scene/heightfield.rs`: opt-in terrain deformation.
- `Shared/Content/scripts/newviso/world/cold.ysc`: shared lifecycle, survival events,
  persisted player state and movement modifiers.
- `Shared/Content/scripts/newviso/gameplay/survival.ysc`: replaceable health,
  frostbite, stamina, moisture, sprint and jump rules.
- `Shared/Content/scripts/newviso/gameplay/colony.ysc`: replaceable resource,
  generator, stress, building, research and law state machines.
- `Shared/Source/environment/winter`: new winter asset authoring inputs.
- `Shared/Source/environment/frostline`: repeatable native conversion of supplied glTF assets.
- `Projects/Frostline`: project policy, procedural generator, game hooks and HUD.

The native engine contains no Frostline project ID, map seed, resource prices,
illness thresholds, cold damage rates, generator levels or law catalogue.
Native numeric bounds protect memory, finite arithmetic and command sizes.
Shared scripts run first; project hooks run afterwards and can override their
commands. Project VFS content can also replace a Shared logical `.ysc` path.
`engine_cold.enabled` is opt-in: ordinary projects retain their movement rules.

## Engine command contract

| Command | Payload |
| --- | --- |
| `snow.field.load` | `asset`, `policy`, `sync_interval` |
| `snow.actor.bind` | `actor`: entity, feet_offset, contact_radius, contact_spacing, contact_strength, max_contact_gap, max_travel |
| `snow.actor.unbind` | `entity` |
| `snow.contact` | `position`, `radius`, `strength` |
| `snow.weather.advance` | delta_seconds, snowfall_metres_per_second, melt_metres_per_second |
| `thermal.environment.set` | `environment`: temperature, wind, wind_loss |
| `thermal.source.upsert` | `source`: id, position, radius, delta_temperature, enabled |
| `thermal.source.remove` | `id` |
| `thermal.body.upsert` | `body`: fields defined by ThermalBody; optional preserve_temperature |
| `thermal.body.remove` | `id` |

`runtime.cold` exposes bound actor snow samples, body temperatures, environment
and sources. It does not transmit the complete snow grid on every frame.
Terrain material `surface_id` is authored data and travels into the exact
triangle collision mesh and contact/audio surface mapping.

Only heightfields authored with `mesh.heightfield.deformable=true` are changed.
The snow grid and terrain vertices must use the same world grid coordinates.
Changes replace immutable vertex blocks, recompute normals and update spatial
bounds. The same triangles are queued to physics before its next fixed step.
Only changed terrain patches are rebuilt. The default coalescing interval is
0.20 real seconds, supplied by the project policy.

Standing still does not stamp or erase tracks. Teleports and airborne movement
do not compact snow. Repeated passage reduces loose depth; weather adds fresh
snow and can refill a trail. Cold checks continue outside snow and while stationary.

## Persistence

The existing checksum/backup/atomic world save has a backwards-compatible `cold`
field. It persists snow compaction, thermal bodies, sources and environment.
Shared survival and project colony state use existing world facts. A changed
terrain asset digest cannot silently reuse incompatible snow. The launcher uses
one ordinary save per seed; changing terrain settings with the same seed requires
a new expedition save.

## New world and launch

Frostline is generated from independent value noise and authoring parameters,
without reading another map. Default extent: 256 x 256 m, 1 m grid, 64 patches,
72 frozen pines, 48 dead trees, a generator clearing and 16 resource nodes.
The forest uses the supplied glTF tree packs, isolated into individual variants.
Camp props are newly authored reusable assets. Trees have buried roots;
spawn, resources and building plots sample the exact terrain triangle diagonal.

Run from the NorthStar workspace:

```bat
Scripts\Run\RunFrostline.cmd
Scripts\Run\RunFrostline.cmd 12345 release
```

The wrapper regenerates the map before launch and builds the selected engine
profile in the separate `NewViso/target-frostline` output directory. Change `Projects/Frostline/config/generator.json` for terrain/biome,
`config/runtime.json` for survival and economy, and `scripts/main.ysc` for rules.
To rebuild the common asset catalogue:

```bat
python Projects\Frostline\tools\generate_world.py
python Shared\Source\environment\frostline\import_assets.py
```

Controls: WASD/mouse, click to capture cursor, Esc to release, Shift to sprint,
Space to jump, V to change view. Hold E near a resource to gather; F near the
generator toggles heating; G toggles overdrive; R purchases a shelter on the next
plot; T starts the next generator research; L enacts the configured law.
Z collects/equips the small axe near its camp placement; X collects/equips the other
axe. Collected tools can be switched anywhere and the same key puts one away.
The held tool follows the first-person camera and swings while gathering wood.
Tool ownership/selection persist; wood efficiency is defined in runtime policy.

## Supplied asset conversion

Input: `Shared/frostline/models` and its referenced `Shared/frostline/textures`.
Original files are never edited. `Shared/Source/environment/frostline/import.json`
owns source groups, material settings, trunk collision radii and tool pivots.
Native packers and the existing RSC7 cooker create resident NEF8 content:

- 147 individual YDRs: 64 fir, 16 oak, 49 pine, 16 high-detail fir and 2 axes.
- 6 YDD dictionaries, with an individual selector for every model.
- 3 YTD dictionaries containing all 7 original images, including normal and
  metallic/roughness maps. Color entries are sRGB; data entries are linear;
  the native texture packer creates mip chains.
- 3 YMAT libraries consumed by the current material gateway, plus 3 NEMAT
  authoring exports. Dictionary material references are logical VFS paths.
- 145 lightweight trunk YBN collision assets (12 triangles each). Tree render
  geometry and collision geometry are independently authored and placed.

Outputs live under `Shared/Content/models/environment/frostline`,
`textures/environment/frostline`, `materials/environment/frostline` and
`environment/frostline`. The latter contains `catalog.json` and `CREDITS.json`.
Trunk assets live under `Shared/Content/collisions/environment/frostline`.
The source author, CC-BY-4.0 attribution and source links are retained.
Rebuildable mesh/binary/material inputs are in `Shared/Intermediate/environment/frostline`.

Node hierarchies are baked, units are metres/Y-up, tree roots are centered using
the lowest trunk section, and tool origins are grip points. Normals use inverse
transpose; negative determinants reverse triangle winding and tangent handedness.
UVs, indices and all original geometry are retained (2,929,085 source triangles).
The installed YDD packer only accepts legacy NEMAT material refs. The import
upgrades the V4 string table to equal-width YMAT logical refs, preserves every
resident offset and geometry byte, then rewraps and revalidates the dictionary
using the native tools. Unknown body versions are rejected. This avoids the
legacy NEMAT gateway being mistaken for the current `material.runtime_v1` contract.
This import does not generate reduced mesh LODs; rendering distance and forest
density remain project settings. Default generation places 120 imported trees.
`frozen_tree_groups`, `dead_tree_groups`, `tree_asset_format` and the per-group
`tree_asset_formats` overrides select the pools
and YDR/YDD addressing; tool placement independently selects either format.
Fir variants use YDD selectors in the default world; other trees and both axes
use YDR. The axes therefore retain their authored tangent streams for normal maps.

`frostline.visual_channels` and each weather phase's `visual_channels` author
timecycle modifiers. The generator derives fog base height from this terrain's
base height, so profiles calibrated for another world's elevation do not hide
the forest. No other project's weather catalogue is changed.

The common renderer accepts `metallic_factor`, `roughness_factor` and a linear
`metallic_roughness` texture slot (G=roughness, B=metallic). The material uniform
opts into this contract; existing materials retain their previous shading.
Both forward and deferred scene shaders read these channels. The renderer's
existing lighting model remains in use; this is not a new reference glTF BRDF.

## Implemented limits

This map is finite, regenerated at launch; it is not an infinite chunk-streaming
world. Snow tracks are grid depressions at 1 m resolution, not detailed boot
silhouettes or volumetric particle snow. Movement resistance is applied by the
shared character controller. Other actors need explicit snow/thermal bindings.
Heat zones are a controllable model, not a CFD/indoor air solver. Shelter wind
and insulation bonuses use a project proximity rule. The prototype population
is an aggregate economy value; individual citizen jobs, illness/treatment,
full food production, construction progress, full law/research trees, scouting,
events, endings and Frostpunk-identical balance are still unimplemented.

## Verification

```bat
cargo test --offline -p newviso-environment -p newviso-scene -p newviso-runtime --lib
python -m unittest discover -s Projects/Frostline/tools -p test_generation.py -v
python -m unittest discover -s Shared/Source/environment/frostline -p test_import.py -v
python Projects/Frostline/tools/verify_playable_runtime.py
```

The integration harness uses the actual TypeScript provider, Vulkan renderer,
and selected physics backend. It temporarily switches only the new project's
entrypoint/save policy, uses an isolated verification save, and restores the
project configuration byte-for-byte with conflict checks. It checks actual
traversal, native compaction, cooling, script rules, tool collection and restart
persistence of trails, temperature and actual player position. Imported asset
streaming failures fail verification rather than silently producing an empty forest.
Verification output belongs to `.newviso/verification` and is reproducible.
The restart harness requires restored state and runs 120 playable frames after
loading. Source/GPU loading frames are not counted as successful gameplay.

Latest verification: 282 Rust tests passed (2 existing tests ignored), 5 procedural
generation and 5 import geometry/format tests passed. The real traversal/restart harness
preserved 14 compacted cells, continued the saved body temperature from 36.958 C
to 36.940 C and retained the actual player position. The global
workspace/documentation and architecture-boundary checkers still report existing
legacy source/dependency/documentation issues outside these new modules.

The verifier accepts `FROSTLINE_VERIFY_ENGINE_EXE` to select a staged release
binary. The current release uses `NewViso/target-frostline/release/newviso.exe`;
this avoids overwriting another running engine executable.
