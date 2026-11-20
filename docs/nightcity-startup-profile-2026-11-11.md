# KaylaNightCity — startup residency blocker audit (2026-11-11)

## Goal

Establish a real gameplay CPU/GPU baseline before selecting HLOD, render-graph,
or Nanite-like optimizations. **Do not substitute UI/platform frames for
gameplay FPS.**

## Bounded measurements (actual local Release run)

| Metric | Run A | Run B |
|---|---:|---:|
| Platform/render frames | 1800 | 600 |
| Wall time | 39.31 s | 22.64 s |
| Gameplay frame samples | 0 | 0 |
| Confirmed map-ready | false | false |
| AssetManager queued | 224 | 237 |
| In-flight asset loads | 8 | 8 |
| Resident assets | 63 | 50 |
| Reported failures | 0 | 0 |

- Run A report: `cache/profiles/live-KaylaNightCity-20261111-124446.json`.
- Run B report: `cache/profiles/live-KaylaNightCity-20261111-125358.json`.

The benchmark autostart path was acknowledged. Therefore this is not the
previous menu-only launch problem.

## Startup readiness breakdown (600-frame run)

Telemetry added in `application_startup_loading.rs` reports:

- 1,290 startup interests / claims.
- Only 24 startup interests resident and materialized at both first and
  last diagnostic samples.
- 1,266 entities pending materialization.
- 237 assets queued, 8 loading, 3 awaiting dependencies.
- GPU readiness is true, `gpu_idle=true`, `gpu_pending=0`,
  `gpu_missing=0`, `gpu_failed=0`.
- `all_claimed=true`, `all_resident=false`, `all_materialized=false`,
  `cpu_idle=false`, `ready_streak=0`.
- Progress was reported unchanged at 0.5582 for 5 spaced diagnostic samples.

**Interpretation:** the bottleneck is on the CPU/source resource loading side.
A live world frame has not advanced. Neither image quality nor gameplay
frame time can be inferred from renderer-only samples.

## Diagnostics now in source

`newviso.perf.startup` emits compact gate counters every 120 startup ticks
only when `NEWVISO_PERF_TRACE=1`.

`newviso.perf.startup.inflight` emits up to 12 in-flight source addresses
with state, effective priority and source bytes (new method
`AssetStreamer::diagnostic_in_flight`). This pinpoints the resources
occupying all decode worker slots without logging the entire catalog.

`profile_live_runtime.py` records `startup_progress` in its JSON report.
Five Python regression tests pass.

## Next investigation

1. Rebuild the Release binary with the in-flight-address diagnostics.
2. Run `python scripts/profile_live_runtime.py --project
   ..\\Projects\\KaylaNightCity --frames 1200 --timeout 120
   --warmup 30 --min-gameplay-samples 120`.
3. Inspect `newviso.perf.startup.inflight` entries for the same asset addresses
   across multiple 120-frame intervals. Check source VFS resolution,
   decoder waits, material dependencies and pathological asset sizes.
4. Fix the blocking source/decoder rather than reducing collision
   readiness or falsely marking the map playable.
5. After a successful fully-resident startup, collect a meaningful warm CPU/GPU
   p95/p99 profile. Only then A/B test HLOD and Vulkan scheduling.

No improvement in actual Night City gameplay FPS is claimed here.
