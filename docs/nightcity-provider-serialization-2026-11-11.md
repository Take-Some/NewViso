# Night City provider serialization — 2026-11-11

**Scope:** current NorthStar/NewViso with installed StarVault and ListFile codec; no Legacy NewEngine SDK was restored.

## Measured provider-call wall time

Pre-host-patch profile: `cache/profiles/live-KaylaNightCity-20261111-153927.json`.
480 renderer frames, 18.08s, 53 resident, 234 queued, map not ready, zero gameplay frames.
Slow calls by YTD source (medians): multilayer maps 3263.7ms (9), MT28 colors 3286.5ms (11), multilayer masks 3295.0ms (10), decals 116.8ms (2).
These are **entire engine.assets call durations**; they do not isolate decompression CPU time.

## Changes built and run in NewViso

- `crates/newviso-host/src/lib.rs`: remove an unnecessary outer service Mutex. ABI trait already requires `ServiceV1: Send + Sync` and `call(&self)`; use `Arc<ServiceV1Dyn>` directly.
- Added concurrency regression proving service calls overlap: 4/4 host tests pass.
- `crates/newviso-rsc7-import/src/lib.rs`: opt-in slow texture decode timing in `NEWVISO_PERF_TRACE`; 23/23 semantic-assets tests pass.
- `cargo check -p newviso-runtime --offline -j 2` and NewViso Release build pass.

Post-host-patch profile: `cache/profiles/live-KaylaNightCity-20261111-154732.json`.
480 renderer frames, 17.14s, 64 resident, 266 queued, 0 failed, map not ready.
Medians: multilayer maps 3155.45ms (9); MT28 colors 1591.09ms (14);
multilayer masks 1556.41ms (18); decals 128.6ms (2).
Some throughput improved but this is **not a definitive A/B speedup**.

## Provider source patch — NOT deployed

Current StarVault provider source `PluginsSrc/AssetManager/newengine-AssetManager/src/module/codecs.rs` held global `codec_registry` mutex across `codec.decode()`.
Patched it to clone its Arc-backed routes under lock and invoke expensive codec work **after releasing the registry lock**. Also patched nested container opening.
Added `codec_registry_lock_is_released_before_decode_dispatch` regression test; formatting checked.
The separately patched `codecs/newengine-codec-listfile/src/nef8.rs` raises the bounded uncompressed NEF8 body-cache budget to 768MiB for a 570,627,572-byte Night City body.

Neither provider nor codec DLL has been rebuilt or installed: the current source Cargo graph still contains stale `NewEngine/neocore2` dependencies. Do **not** restore Legacy SDK. Migrating build contracts to supported NewViso APIs, testing, Release publishing, and verifying DLL hashes/stamps are necessary first.

The runtime's map/collision readiness rules were unchanged. No valid gameplay CPU/GPU p95/p99 or FPS exists until `map_ready=true`.
