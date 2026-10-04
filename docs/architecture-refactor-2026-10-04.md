# Navigation and renderer architecture pass — 2026-10-04

The pass changes the existing working tree in NewViso. It preserves prior edits,
provider protocols, public navigation APIs, and project-owned behavior. It does
not claim to complete the repository-wide monolith or duplication cleanup.

## Reference architecture

The inspected reference is `C:\Games\gta v source code\GTAV Source\src\dev_ng\game`.
`pathserver/PathServer_PathRequest.cpp` and `PathServer_PathStringPull.cpp` separate
request processing from corridor refinement; `renderer/RenderListBuilder.h` and
`renderer/DrawLists/drawlistMgr.h` separate submission construction from execution.
`scene/EntityBatch.cpp` keeps instance batching and bounds as explicit responsibilities.
The changes apply those separation principles to NewViso's existing contracts;
they do not transplant source-format, project, or platform policy into the host.

## Ownership

| Area | Module | Responsibility |
| --- | --- | --- |
| Navigation | `types.rs` | Public descriptions and validation |
| Navigation | `tiles.rs` | Incremental ingestion, replacement and removal |
| Navigation | `connectivity.rs` | Welded edge graph and off-mesh links |
| Navigation | `search.rs` | Request queue, A*, obstacles and endpoint selection |
| Navigation | `geometry.rs` | Triangle queries, corridor reconstruction and funnel |
| Navigation | `spatial.rs` | Persistent XZ broad phase for polygon candidates |
| Renderer | `renderer_init.rs`, `renderer_resources.rs` | Composition and frame-ring resource creation |
| Renderer | `renderer_pipelines.rs` | Shader and pipeline suite construction |
| Renderer | `renderer_gpu_geometry.rs` | Geometry capacity and static/skinned uploads |
| Renderer | `renderer_materials.rs` | Texture/material residency and bind groups |
| Renderer | `renderer_asset_submission.rs` | Visible instance runs and opaque/Hi-Z submission |
| Renderer | `renderer_transparent_submission.rs` | Transparent range centers and draw collection |
| Renderer | `renderer_submission.rs` | Compact command packing, run iterators and binding cache |
| Renderer | `renderer_graph.rs`, `renderer_passes.rs` | Graph declaration and pass recording |
| Renderer | `renderer_frame.rs` | Frame preparation, acquisition, timing and recovery |
| Renderer | `renderer_sky.rs`, `renderer_atmosphere.rs` | Sky/cloud GPU resources |
| Renderer | `renderer_shutdown.rs` | Ordered GPU teardown |
| Living world | `runtime_navigation.rs` | Shared completion for graph-routed coarse/physical travel |

Navigation `lib.rs` goes from 1,165 total lines to 162. Renderer initialization
goes from 2,163 lines to 358, and frame orchestration from 1,870 lines to 428
production lines. The largest extracted renderer implementation is 542 lines.
There are 20 new module files and 26 changed/new Rust files.

## Behavior and performance

The polygon index stores each triangle in every cell touched by its XZ AABB.
Candidate selection still uses exact 3D closest-point distance, the same snap
limit, obstacle rules, and deterministic polygon-ID tie breaks. Oversized faces
are always considered. Large query windows fall back to the original complete
scan. Partially built tiles are indexed immediately and removed atomically with
their polygons. The index is derived state, not a new asset or serialization format.

The renderer shares invariant pipeline descriptions and material/vertex binding
state. Visible batch runs are yielded by an iterator instead of allocating a
temporary run vector. Existing material order, transparent depth sorting,
main-view overrides, shadow coverage and Hi-Z fail-open behavior are retained.

An acquired frame now aborts on errors during uploads, lighting, dashboard/cloud
preparation, draw recording and finalization. Previously only errors from the
inner draw routine triggered this cleanup. Frame-ring allocation cleans up a
partial allocation, and scene/shadow capacity growth commits only after both
replacement rings are ready.

LivingWorld shares one completion implementation for graph-routed coarse travel
and externally controlled physical actors. A regression checks that both emit one completion
with the requested payload. The scheduled-event test now drains fixed-step backlog
before expecting its one-second deadline; production clock behavior is unchanged.
Concurrent coordinate-goal additions were preserved and included in final workspace
verification; they are outside this pass's graph-travel completion refactor.

## Measured endpoint snapping

Same machine, release profile, one resident tile, two triangles per isolated
2x2 m square at 12 m spacing. Each sample runs 512 deterministic queries with
start=end, checking `Found`; the table reports the median of five samples. This
isolates endpoint lookup, not long-route A*, streamed tile construction, GPU
performance, or end-to-end game frame rate.

| Resident polygons | Before, µs/query | After, µs/query | Speedup |
| ---: | ---: | ---: | ---: |
| 2,048 | 24.315 | 0.764 | 31.8x |
| 8,192 | 107.444 | 0.922 | 116.5x |
| 32,768 | 455.339 | 1.026 | 443.8x |

Reproduce the current implementation from the repository root:

```powershell
python scripts/benchmark_navigation.py
```

The benchmark creates its disposable Rust harness under `.newviso/benchmarks`;
the versioned script uses the public API and does not require a renderer/provider.

## Verification

- `cargo check --workspace --all-targets --offline --locked`: pass.
- `cargo test --workspace --offline --locked`: 497 passed, 0 failed, 1 ignored.
- `cargo build --release -p newviso --bin newviso --offline --locked`: pass.
- `rustfmt --edition 2021 --config skip_children=true --check` on changed Rust files: pass.
- `git diff --check` on the changed tracked files: pass.
- Disposable primitive/sky scene with deployed Vulkan provider: 120 frames,
  eight graph passes executed, exit 0, in normal mode without UI and in safe mode.
- Public search behavior is compared against brute force for negative coordinates,
  stacked floors, blockers, ties, several snap limits, oversized faces and partial
  tile replacement. Candidate work remains local with distant geometry present.

The release executable is `target/release/newviso.exe`. No Rust/Cargo source drift
was detected between the final validation start and completion.

## Remaining findings

The existing repository checks still report unrelated accumulated violations:
`verify_boundaries.py` reports 32, `verify_no_console_output.py` reports 6, and
`verify_code_health.py` reports 244. Their policies were not relaxed. The code-health
scanner reports 36 oversized modules in the final concurrent working tree. This
pass removes three oversized modules (navigation, renderer initialization and
renderer frame); its immediate count decreased from 40 to 37. Identical eight-
significant-line windows decreased from 189 to 148. Window counts overlap and are not counts of
independent duplicated functions. The scanner itself does not fully distinguish
all test-only code and should not be treated as a complete semantic architecture audit.
`verify_isolation.py` passes.

A normal UI-enabled smoke is blocked by the installed renderer DLL rejecting
the host's existing `SetUiDrawList` command. The untouched First3D template also
references an old UI source format and declares an overlay budget smaller than
its inherited Shared UI requires. The successful temporary scene omits that UI
capability, disables the Shared player, and raises its overlay budget. Original
project files and provider DLLs were not changed.

The normal no-UI smoke additionally logged a missing compiler-output temporary
file for asynchronous Hi-Z shader compilation. It still completed all 120 frames;
this smoke therefore does not establish Hi-Z correctness on a populated asset map.
