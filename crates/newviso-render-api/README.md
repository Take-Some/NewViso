# newengine-render-api

Stable render contract shared by the runtime host, renderer plugins and render-frame-graph builders.

## Service boundary

Renderer plugins expose one service id and two JSON methods:

- service id: `render.api`
- `info_json` — returns `RenderBackendInfo`
- `invoke_json` — accepts `RenderServiceRequest` and returns `RenderServiceResponse`

The runtime submits complete frames through `RenderFrameEnvelope`: frame index, clear color, surface extent, viewport extent, graph, draw-list declarations, ordered `UiLayerDrawPacketSet` and optional work budget travel together as one backend-facing packet. Render provider ABI v2 intentionally has no singleton UI command or UI draw-list kind.

## Modules

- `protocol` — `RenderBackendInfo`, `RenderCommand`, `RenderServiceRequest`, `RenderServiceResponse`, `RenderFrameEnvelope` and JSON helpers.
- `frame_graph` — declarative pass/resource graph descriptions, validation, barriers and submit reports.
- `draw_lists` — typed draw-list routes and replay statistics.
- `provider_bridge` — stable external draw-list/light extraction provider contracts.
- `diagnostics` — backend capabilities, residency, upload pacing and runtime diagnostics.

The renderer contract intentionally has no versioned fallback endpoints. Renderer/plugin/runtime mismatches must be fixed by rebuilding and resyncing the matching module set.

Render protocol v2 uses strict-major negotiation: clients and providers must have the same `RenderApiVersion.major`. Minor/patch differences may select the backend stable version, but v1 and v3 peers are rejected by a v2 backend before `RenderApiRef` is published.

## PostFX and shadow feature contract

`PostFxFrameParams` now carries explicit quality DTOs for the provider-owned post-processing chain:

- HDR exposure and tone mapping;
- tunable bloom threshold/knee/intensity/radius;
- FXAA edge/subpixel filtering;
- color grade, local contrast, vignette and deterministic dithering;
- sun disk, lens flare and radial ray parameters.

The default payload remains backward-compatible: old frame envelopes that only provide `display` and `sun` deserialize with production defaults for `quality`.

Shadow capability metadata is expected to describe the implemented provider surface. The current Vulkan path supports directional depth shadows, light-frustum caster culling, `normal_bias` through the lit UBO and PCSS-style receiver filtering. Atlas/true multi-cascade rendering remains a graph-level extension and must not be emulated by engine-side fallback code.

<!-- NORTHSTAR-DIR-README:BEGIN -->

## Directory purpose

**Path:** `NewEngine/neocore2/crates/newengine-render-api`

**Role:** Engine repository directory. Keep files here scoped to this directory's domain and avoid cross-domain ownership drift.

**Local contents:** 1 direct subdirectories, 2 direct files.

**Direct file examples:** `Cargo.toml`

## Working rules

- Do not put transient build output in this directory unless the directory is explicitly a runtime output/cache location.
- Keep runtime assets and editable source assets separate: source assets are packed into runtime formats through explicit tools/manifests.
- Do not introduce hidden provider/backend coupling here; use declared descriptors, gateways, DTOs, and explicit maintenance scripts.

<!-- NORTHSTAR-DIR-README:END -->
