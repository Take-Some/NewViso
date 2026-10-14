# Shared town assets and Island layout

Island is now a coastal town with 11 buildings, two north/south streets,
three cross streets, a central square and a graded road to the shore.
The authored town platform is 5.4 m above sea level. A 129 x 129 heightfield
covers 224 x 224 m and keeps continuous beaches and underwater terrain.
All six house variants have authored roofs, windows, doors and porches.
The two-storey variants include balconies; the cafe has an awning.

## Ownership and authoring

Reusable buildings, furniture, imported street props and materials belong
to Shared. Their authoring entry is
Shared/Source/environment/town/town_assets.py. Source inputs, original
texture dictionaries, material definitions and provenance are stored beside
it. Cooking uses the common Tools/native_rsc7_cooker/bin installation.
No other project folder is required to rebuild these common resources.

The runtime catalog is Shared/Content/environment/town/catalog.json.
It contains six building prefabs, one square planter and nine imported props.
Runtime meshes and collision files live under models/environment/town and
collisions/environment/town. Texture dictionaries live under
textures/environment/town. Existing corrected vegetation remains under
models/environment/vegetation/gtav.

Island owns config/town.layout.xml, its heightfield, shoreline regions,
placement data and streets.ydr/streets.ybn. The project generator creates
road markings, crossing stripes and raised sidewalks with curb ramps.
Paint decals cast no shadows and add no physical obstacles; the road,
sidewalk and curb triangles are shared by visual and collision authoring.
Buildings use exact triangle collision. Lamp posts use a tight pole
collider instead of a bounding box around the elevated arm.

The generator also writes terrain_spawn.ysc and town_water_spawns.ysc.
Scene and gameplay physics therefore use the same spawn positions.
Floating cargo stays offshore after the island terrain change.
The existing build_island_biome.py entry delegates to the town generator
when town.layout.xml is present.

## Lighting and rendering

Shared/Content/scripts/newviso/world/street_lighting.ysc is installed by
the common default_game lifecycle. Projects opt in with
runtime.variables.engine_lighting; absent configuration is a no-op.
Shared/Content/config/environment/street_lighting.defaults.xml supplies
the authoring defaults.

The controller accepts native camera vectors {x,y,z} and array vectors.
It applies the native climate day_night_ratio, selects nearby lamp heads,
uses separate activation/deactivation distances and updates every 0.25 s.
Only changed intensities generate scene commands. Twelve active local
sources leave forward light slots for other effects. Lamp geometry remains
visible throughout the town; intensity is zero during daylight.
Island has 30 authored lamps, including four around the square.

Authored town material slots explicitly request use_vertex_color=true.
CpuAssetMaterial converts that boolean to the existing shader flag 64.
Imported materials default to false, preserving their source vertex-mask
behavior and the separate terrain domain flag 256. No new shader layout or
model-name special case is introduced.

## Verification

The material regression checks true, false and absent use_vertex_color.
The release engine builds successfully.

Native camera runs inspect the actual assets from above, street level,
the square and shore in daylight, and the streets and square at night.
A native script contract checks daylight-off behavior, object camera
coordinates, movement, the light budget, stable updates and absent
configuration. The earlier camera-coordinate fault is fixed.

Four native capsule probes test walking across a road junction,
blocking by a house wall, the graded coastal road and blocking by a lamp
pole. The scene installs 14 streamed triangle mesh colliders.
The static asset audit checks finite vertex streams, valid indices,
nondegenerate triangles, collision stream sizes, explicit tint policy and
texture-file references.

The native daytime and corrected nighttime runs load 221 resources
(approximately 59.2 MiB), with zero failed loads, no waiting dependencies
and no budget overflow. Main-view character reflection exclusion remains
active. Installed-executable smoke results, images, asset hashes and
binary installation metadata and final report (final-island-town-validation.json) are recorded under
Tools/tmp/island-town-work. This change does not extend the water fidelity
claims documented in water-reference-port.md.

## Final installed checks

| Run | Frames | Exit | Logged errors |
| --- | ---: | ---: | ---: |
| Installed Island, normal project entry | 900 | 0 | 0 |
| Installed Island, gameplay capture after intro | 450 | 0 | 0 |
| Installed scene without water or town lighting | 180 | 0 | 0 |

The final town scene contains 186 authored entities and 169 model instances.
Its 11 houses, streets, square and 30 lamps are active in the normal Island
project entry. The player capsule remains at center [-8, 6.485, 21] on the
square pavement. The actual gameplay capture is town-installed-island-view.jpg;
it is taken after the intro closes.

## Rebuild

From Shared/Source/environment/town:

    python town_assets.py

From Projects/Island:

    python tools/build_island_town.py

From NewViso:

    cargo build --release --bin newviso --offline
