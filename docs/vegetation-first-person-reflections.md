# Vegetation materials and first-person reflections

The affected palms and bushes had two independent rendering faults. Imported source shadow-only models were submitted in visible passes, and the common heightfield helper mistook imported vertex alpha for a terrain material tag. The latter interpreted generic normal/specular bindings as terrain color layers.

MeshResource now carries source-neutral render_passes: {color, shadow}. Older semantic assets default to both passes; malformed explicit flags are rejected. Native drawable and fragment authoring preserves the source mask and emits normalized flags. Color draw ranges omit shadow-only geometry; shadow ranges use the source shadow policy. Joint partitioning retains these flags. The five original vegetation drawables contain visible masks 0xFD and shadow proxy masks 0xE2. No model-name or shader-name lookup is needed by the runtime.

Heightfield layers now require material domain bit 256. The terrain material owns its uniform buffer and combines that bit with vertex color bit 64. Imported alpha no longer selects heightfield samplers. This works in the forward and GBuffer scene shaders, including water optical draws.

## Shared content

The three palm models, two bushes and eight texture dictionaries live under Shared/Content/models/environment/vegetation/gtav and Shared/Content/textures/environment/vegetation/gtav. All 55 Island placements use these common paths.

Original RSC7 inputs, byte-preserved cooked texture dictionaries, the source manifest and rebuild.py live under Shared/Source/environment/vegetation. Intermediate NVRM bodies and triangle/texture validation live under Shared/Intermediate/environment/vegetation. Visible vertex payloads remain unchanged. There are zero untextured color-pass triangles. The common cooker installation is under Tools/native_rsc7_cooker/bin; an explicit --cooker argument can select another installation without requiring a project folder.

## Camera-local geometry

An instance with main-view hidden geometry, or a main-view-only instance, is excluded from reflection/refraction/depth optical targets. Mixed instance batches retain their other members. The head/body main-view policy continues to retain visible arms/body and full world shadow geometry.

The sign of the affine instance matrix homogeneous lane transports the reflection policy without changing the instance layout. Instanced scene, atmospheric-depth and shadow shaders restore +1 before transforming positions. The scene shader outputs negative GBuffer normal coverage for excluded instances; deferred lighting uses absolute coverage to preserve their normal lighting. GBuffer normals are Rgba16Float.

SSR rejects excluded geometry both as a ray origin and as a trace hit. It checks the discrete four-texel bilinear footprint at source, candidate and refined hit, preventing silhouette color leakage. SSR projection/reconstruction also uses the same Vulkan Y orientation as the scene camera. Clearing the first-person override restores ordinary reflection eligibility; static instance tables refresh only when that eligibility changes.

## Verification

The semantic decoder tests cover default and explicit mesh pass flags and reject malformed flags. Three main-view visibility tests include instance-local, reversible reflection exclusion. The actual mixed head/hand partition test preserves every triangle, material and render flag. A heightfield uniform regression checks explicit domain selection.

All 35 packaged scene shaders compiled. The changed provider SSR/deferred shaders passed strict compilation. Engine and provider release builds succeeded. The native cooker release build and imports of all five original drawables verified masks 0xFD/0xE2 on every resulting mesh. Rebuilding the Shared bundle through the common Tools installation reproduced every validated runtime hash.

| Native run | Frames | Exit | Logged errors |
| --- | ---: | ---: | ---: |
| Actual Island content and player, nine camera stages | 3600 | 0 | 0 |
| Installed executable, actual Island entry | 900 | 0 | 0 |
| Installed executable, scene without water | 180 | 0 | 0 |

The Island runs had 157 resident resources, zero failed loads, zero waiting dependencies and no budget overflow. The player snapshot reports reflections_visible false while preserving 165639 visible main-view vertices. Actual native window captures cover both bush models, palm/shoreline views, first-person views, reflection/refraction and immersion. The dedicated medium-palm view is partly obstructed by terrain; the wide shoreline views and complete texture/mask validation provide additional coverage.

The installed executable and provider hashes, build/test results, capture paths and full validation report are recorded at Tools/tmp/water-work/final-vegetation-reflection-validation.json. Original files and binaries are backed up there. Existing water fidelity boundaries remain documented in water-reference-port.md; these checks do not establish pixel-identical GTA V output.
