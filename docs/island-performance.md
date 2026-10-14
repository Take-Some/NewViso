# Island performance audit — 2026-10-11

The current production scene includes the original GTA settlement transfer:
880 entities, five NPCs, 666 authored settlement instances, 148 distinct imported
models and 105068 collision triangles across the two parcels. The expanded map
currently measures 67.7–78.8 FPS on average in the controlled scenarios below.
Neither a strict 60 FPS minimum nor stable 120 FPS is established; walking still
has intervals above the 16.67 ms budget. Earlier 127–134 FPS measurements below
refer to the smaller scene before this transfer.

## Original settlement release

Same Quadro RTX 4000 and 1440x900 viewport, fixed 13:00, 60-second configured
runs, first 400 presented frames excluded. GPU samples with frame indices inside
the warmup are also excluded, even if delayed readback is logged later. GPU
sample counts are small (six or seven); their percentiles do not establish a
long-run guarantee.

| Scenario | Mean observed FPS | CPU median / p95 / p99, ms | GPU span median / p95, ms | Intervals above 16.67 ms |
|---|---:|---:|---:|---:|
| motel idle | 71.09 | 13.70 / 14.77 / 15.37 | 10.69 / 10.85 | 17/3821 |
| residential idle | 78.85 | 12.35 / 13.24 / 13.88 | 8.98 / 9.12 | 10/4294 |
| motel walking | 67.69 | 13.99 / 16.49 / 17.59 | 10.07 / 13.03 | 399/3624 |
| motel THUNDER | 68.56 | 14.20 / 15.36 / 16.02 | 10.97 / 11.08 | 34/3669 |

All four runs exited successfully with zero runtime errors and zero failed
streaming assets. The walking controller displaced about 21 metres and ended
standing at the original motel ground beside a collision boundary. This checks
the exercised path; it is not a complete walk-through of every room or staircase.

The first dense motel probe measured 29.91 FPS. Its fragment-presentation frame
snapshot contained about 1.16 MB, repeatedly rebuilt and passed to both script
modules. Opt-in generic snapshot deltas now send immutable descriptions once,
retain per-root read-only script state and resynchronize after reload. The
engine retains frame-state ownership, borrows comparison paths, skips equal
subtrees and moves nested JSON values directly into the final snapshot. It also
caches fragment mesh bounds and invalidates them before vertex changes.

Motel snapshot construction fell to a median 2.86 ms; the remaining scripting
phase measured 5.23 ms and full CPU orchestration 13.70 ms. All 93 complete
fragment-presentation records from the final idle state matched the initial
dense-scene probe exactly. Imported geometry, texture dimensions/mips, NPC
count, shadow settings, rain and water simulation were not reduced.

The original parcel layout, all 666 source transforms, native texture closure,
source hashes, collision indices and tiled terrain joins pass four offline
import regression checks. The release build passes 12 project, 9 scripting,
121 runtime and 137 scene tests (two scene tests remain ignored). The deployed
TypeScript provider also passed the decoder integration check in each of the
four runs. Current traces are under:

- Tools/tmp/island-profile-original-release-motel-89b4c6ad
- Tools/tmp/island-profile-original-release-trailers-ac4dee48
- Tools/tmp/island-profile-original-release-motel-walk-80b7ac72
- Tools/tmp/island-profile-original-release-motel-thunder-f31783b1

Shared/Source/environment/settlements/README.md documents provenance, rebuild
commands, common asset paths and visual fidelity limits. Original map assets
are preserved; current engine lighting, post-processing and translated material
shaders do not reproduce GTA V pixel-for-pixel. Further work toward 120 FPS
must address the remaining scripting cost and dense-scene GPU passes without
treating the old smaller-map result as the new production result.

The canonical Scripts/Run/RunEngine.cmd Island release command launched the
current project successfully. Its window also survived minimize/restore and
continued rendering with no errors in the current process session. The shared
current log retains earlier launches; historical failures are excluded using
the current process start timestamp. No legacy generated ruin entities remain
in the authored Island scene.

## Earlier smaller-map release

Device: Quadro RTX 4000, 7995 MiB dedicated VRAM, Vulkan 1.4.329.
Viewport: 1440x900. Fixed 13:00 clock, CLEAR or THUNDER. The source scene is
the five-NPC Island snapshot frozen under
Tools/tmp/island-profile-120-final-idle-14ce0bb1. Synthetic movement holds W
and turns at six degrees per second without capturing the operator pointer.
Configured run windows are 60 seconds, except the 35-second linear control.
The first 400 presented playable frames are excluded. Loading/suspended frames
are excluded and reset interval continuity. GPU query results are read only
after their frame slot completes, without a blocking query wait.

| Scenario | Mean observed FPS | CPU median / p95 / p99, ms | GPU span median / p95, ms | Water GPU median, ms | Intervals above 16.67 ms |
|---|---:|---:|---:|---:|---:|
| idle | 132.73 | 7.14 / 7.89 / 8.59 | 7.25 / 8.09 | 0.076 | 4/5923 |
| walk | 133.97 | 7.04 / 7.77 / 8.36 | 7.38 / 8.18 | 0.071 | 4/3566 |
| thunder | 127.01 | 7.45 / 8.53 / 9.10 | 7.30 / 8.67 | 0.074 | 4/7139 |
| linear-ab | 105.87 | 9.11 / 9.74 / 10.00 | 9.46 / 9.97 | 2.075 | 0/3252 |

The linear control uses the same executable, provider and content, with only
NEWVISO_LINEAR_WATER=1 restoring expanded water vertices. The indexed path
retains all 390186 corners as indices, using 65560 bit-identical unique
vertices. Triangle order, winding, UV, colour, tangent and near/far modes are
preserved. The physical grid remains 256x256, two metres per cell, with 40
authored wave regions. Physics remains 120 Hz, maximum eight substeps.
Reflections, refraction, rain, shadow resolution, geometry and NPCs stay enabled.

Observed FPS is the reciprocal of the mean interval between consecutive
presented-frame log timestamps. Those timestamps have millisecond precision;
individual interval budget counts are approximate and are not a strict
presentation SLA. GPU span is a real timestamp interval from the first graph
through final frame submission. It is not the display scanout time.

Before water indexing, the same frozen current scene reached 108.70 FPS idle,
113.32 walking and 109.76 in THUNDER. An earlier, smaller Island snapshot had
reached approximately 50 FPS before GPU skinning and command-path work; those
different-content results must not be treated as a controlled map comparison.

## GPU evidence

A sampled pre-index frame was approximately 8.98 ms. Its water surface/shore
phase consumed 2.06 ms, while ordinary transparency consumed only 0.26 ms.
Additional target medians were approximately 0.83 ms for shadows, 0.64 ms for
cloud soft depth, 0.56 ms for planar reflection, 0.90 ms for refraction and
0.09 ms for refraction depth. Target timings plus graph categories account
for approximately the whole measured frame span.

Expanded water geometry repeated the vertex shader's field reads for every
triangle corner. Exact vertex indexing allows the GPU vertex cache to reuse
those results. The controlled linear fallback again costs approximately
2.07 ms in the water phase, confirming the cause independently of CPU changes.

## Common implementation

- GPU skinning uploads immutable bind geometry once per source model and small
  per-frame palettes/uniforms. A three-slot GPU output ring supplies the existing
  17-float geometry layout to main, shadow and optical passes. Full rig, eight
  influences, transitions, physical pose, IK and feet adjustments remain active.
  Near GPU pose sampling runs at 120 Hz; existing far cadences remain adaptive.
- Exact corner keys preserve attributes. Dense meshes use a linear GPU input;
  repeated corners are deformed once and scattered. Same logical assets replaced
  with a new source allocation invalidate model input and every dependent group.
- Static terrain is retained in immutable Arc blocks. Per-slot upload tracking
  skips unchanged prefixes and uploads only changed tails. Failed uploads do not
  commit cache state; moved blocks and recreated buffers invalidate offsets.
  Walking geometry preparation fell from 1.56 ms to 0.02 ms.
- Ordered typed render recording sends existing NECB v2 binary packets for
  supported state/draw/compute commands. Control scopes remain ordered JSON.
  Immediate allocations, queries, uploads and graph submissions flush prior work.
  Unsupported binary methods fall back only before a packet can execute.
- The provider exposes opt-in full-frame, graph-category and labelled target GPU
  timestamps through renderer_inspector_v1. Buffer/barrier contracts allow compute
  outputs to be consumed safely as vertex attributes.
- A minimized native 0x0 surface defers swapchain recreation and preserves the
  running process until restoration. Device-loss and other errors remain errors.
- Character angular velocity is carried by the existing physics body snapshot,
  avoiding an unsupported installed-provider command. This is a character-path
  compatibility fix, not a claim that the entire physics SDK migration is complete.

The earlier owned upload packets, water solver scratch/axis reuse, per-slot field
revisions, startup animation preparation and Shared water/particle preparation
remain in place. All engine rendering changes apply to other projects.

## Validation

The current release build succeeds. Enabled test counts: 137 scene, 121 runtime,
4 render client, 58 render API and 152 Vulkan provider; two scene and three
provider tests are ignored. Test suites were run when their corresponding code
changed. The opt-in real Quadro compute readback test passes in both linear and
scatter skinning modes, including guard regions, eight weights and all 17 output
channels against an independent CPU reference.

The water indexing test reconstructs every corner of a full 256x256 grid and
checks exact channel bits, seams, signed zero, NaN payloads and malformed input.
Source replacement and upload cache retirement/failure cases are also covered.

Final performance probes have no runtime errors or streaming failures. A separate
client-area visual probe survives minimize/restore and exits cleanly. Canonical
Vulkan 0.37.7 publishing passes export signature/root and build-path audits.

General workspace documentation and architecture verification still fail on
pre-existing migration/dependency/documentation debt. No full-workspace green
claim is made. Scoped whitespace checks pass and source patch rejects/backups
are absent.

## Remaining frame risks

CPU maximums of 25–49 ms remain in the final runs. The cold playable frame also
retains first physics/render work and is reported separately in each summary.
Strict 120 FPS requires addressing these stalls and the THUNDER tail, followed
by longer traversal/night/interaction tests on the actual expanded scene.

## Reproduction

From NewViso:

    python scripts/profile_island.py --label idle --scenario idle --weather CLEAR --seconds 60 --frames 18000
    python scripts/profile_island.py --label walk --scenario walk --weather CLEAR --seconds 60 --frames 18000
    python scripts/profile_island.py --label storm --scenario walk --weather THUNDER --seconds 60 --frames 18000

Set NEWENGINE_VULKAN_GPU_TIMESTAMPS=1 and
NEWENGINE_VULKAN_GPU_TIMESTAMP_INTERVAL_FRAMES=60 for GPU samples.
NEWVISO_CPU_SKINNING=1, NEWVISO_JSON_RENDER_BATCH=1 and
NEWVISO_LINEAR_WATER=1 restore individual comparison paths.

Evidence:

- Tools/tmp/island-profile-120-index-idle-60a8aaac
- Tools/tmp/island-profile-120-index-walk-ef798bb8
- Tools/tmp/island-profile-120-index-thunder-d614ae8b
- Tools/tmp/island-profile-120-index-linear-ab-9080ad38
- Tools/tmp/island-profile-120-target-timing-2b5b59d8
- Tools/tmp/island-120-complete-build.log
- Tools/tmp/island-120-index-build.log
- Tools/tmp/island-120-target-build.log
- Tools/tmp/island-120-restore-smoke.log
- Tools/tmp/island-120-workspace-checks.log
