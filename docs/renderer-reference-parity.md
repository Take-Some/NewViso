# Renderer reference comparison — 0.34.0

Date: 2026-09-29. Scope: NewViso scene renderer and the external Vulkan provider.
This release fixes concrete rendering and scheduling faults. It does not establish
full feature, image, or performance parity with the reference.

## Authorities reviewed

Reference files below are relative to the supplied GTAV Source/src/dev_ng/game/renderer:
- RenderListBuilder.cpp: material render buckets and alpha-test state.
- RenderPhases/RenderPhaseStd.cpp: distinct geometry and alpha draw-list phases.
- Deferred/GBuffer.cpp: multiple material/depth render targets.
- Lights/TiledLighting.cpp: tiled light classification and submission.
- PostProcessFX.cpp: HDR, luminance/exposure and postprocessing state.

Implementation authorities:
- NewViso/crates/newviso-scene/src/first_scene/renderer_frame.rs
- NewViso/crates/newviso-scene/src/first_scene/renderer_init.rs
- NewViso/crates/newviso-scene/src/assets/scene.frag
- PluginsSrc/VulkanRenderer/newengine-modules-render-vulkan-ash/src/render_api/

The reference is used for comparison of responsibilities and behavior; the changes
are native Rust/GLSL implementations within the existing NewViso interfaces.

## Changes in this release

### Material coverage in atmospheric depth

Both authored-cloud and volumetric-cloud depth passes now use the instanced depth
fragment shader with the existing material binding layout. Texture alpha, material
opacity, alpha cutoff, and optional vertex alpha follow the visible scene shader.
Alpha-blended surfaces remain excluded by the existing opaque/cutout draw selection.

Before this change, both paths rasterized entire cutout triangles into linear depth.
A tree card could therefore hide the clouds behind its transparent texels.

The material layout is created before cloud pipelines. Each cloud owner retains and
destroys its additional fragment shader. Changed pipeline cache keys use v2.
Scene and both depth consumers share one material-run iterator. It respects both
material boundaries and indirect-buffer offset discontinuities, without allocating
a separate batch vector. Instanced and indirect rendering remain active; depth now
requires one native batch per material instead of the previous incorrect
material-blind single batch. This is a correctness tradeoff, not a claimed CPU gain.

### Dry-weather shading

The surface fragment shader skips puddle-layout and puddle-normal sampling and
ripple-normal work when accumulated wetness is zero. The branch is draw-uniform,
so implicit texture derivatives remain valid. Wet-weather equations are unchanged.
This removes two authored texture lookups per dry surface fragment. No FPS gain is
claimed without a fixed-camera, same-resolution A/B measurement.

### Phase routing and deterministic fallback flush

Setting a compatible draw-list category preserves an already selected GBuffer,
DepthPrepass, or ShadowCascadeMap phase. Incompatible category changes and explicit
None still select the legacy default. Previously, setting OpaqueForward after
GBuffer silently redirected the commands to ForwardOpaque.

Fallback phase order now includes VisibilityCull before geometry, plus FroxelFog
and ScreenSpaceReflections before PostFx. These phases previously fell through to
unordered HashMap selection after the listed phases.

### GPU measurement correctness

Timestamp intervals use wrapping subtraction at the graphics queue's actual
timestampValidBits width. Upper non-counter bits are masked. A fixed 64-entry
scratch array replaces per-sample heap allocation, and older completed samples
cannot overwrite a newer published frame. Reads remain nonblocking.
This changes measurement correctness, not GPU execution time.

### Version and compatibility

Vulkan provider: 0.33.8 -> 0.34.0. Exported provider version derives from Cargo.
The render service ABI and wire schema are unchanged. NewViso's general workspace
version remains 0.1.0; the requested renderer version is independently owned.

## Remaining integration gaps

| Area | Current evidence | Required acceptance work |
| --- | --- | --- |
| Main frame | Active scene graph uses forward opaque and transparency. | Move compatible scene geometry through real GBuffer/deferred passes while preserving material parity and fallback coverage. |
| Lighting | Active scene fragment uses a bounded 16-light uniform array; provider has separate deferred/light infrastructure. | Connect scene light extraction to the tiled/clustered path and verify many-light scenes. |
| HDR/postprocessing | Scene pipelines target Bgra8Unorm; provider has separate postprocessing modules. | Establish linear HDR scene color, exposure, tone mapping and final display transfer; verify bloom and alpha ordering. |
| Shadows | Active scene owns one directional shadow map. | Integrate scene casters/receivers with cascades and local shadow atlas, including cutout and skinned coverage. |
| Atmospheric depth | Material coverage corrected in this release. | Dedicated foliage/fence comparison at equal camera, projection and resolution; assess low-resolution edge artifacts. |
| Reflections, water, SSAO, temporal AA | Source modules in the provider are not proof of active scene fidelity. | Verify contracts, actual graph execution and reference captures separately. |
| CPU parallelism | Existing worker recording is opt-in. | Measure scene-level image equivalence and frame-time distributions before changing defaults. |
| Performance | This release removes dry-weather work and diagnostic allocation. | Fixed workload median/p95/p99 CPU and GPU measurements, with validation and streaming stability. |

## Verification

- Packaged scene GLSL compilation: passed with the bundled glslang compiler.
- Scene library tests: 64 passed, one manual stress test ignored.
- Vulkan provider library tests: 136 passed, three opt-in hardware tests ignored.
- NewViso host cargo check: passed.
- NewViso release build: passed.
- Provider publication and runtime smoke: recorded after execution below.

Validation logs and exact pre-edit source/runtime backups are retained under the
engine's .newviso working state. Do not interpret ignored hardware tests, shader
compilation, or a startup smoke as pixel-equivalence or an FPS benchmark.
