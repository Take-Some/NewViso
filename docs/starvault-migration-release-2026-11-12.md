# StarVault 3.8.1 / ListFile 0.1.1 migration and release record

**Date:** 2026-11-12  
**Root:** `C:\Users\Kayla\Documents\NorthStar`

## Current architecture and outcome

- Provider `engine.assets.starvault`, concrete service `asset_manager.api`, routed under `engine.assets`.
- The updated source workspace `PluginsSrc/AssetManager` depends on current NewViso crates:
  `newviso-plugin-kit` (which reexports `newviso-plugin-abi`),
  `newviso-asset-interfaces`, `newviso-service-api`,
  `newviso-binary-asset-bodies`, `newviso-texture-container`,
  `newviso-model-domain`, `newviso-task-api`, and current maps/XML crates.
  No retired `NewEngine/neocore2` SDK was restored.
- Loaded successfully by the NewViso host's `newviso-compat-abi` boundary;
  the provider advertises the existing 54-method `newengine.assets-api.v1`
  service contract. The service contract string remains a wire-compatibility
  identity; it does **not** mean runtime execution uses Legacy NewEngine.
- Source and installed runtime preserve VFS compiled-first/source fallback,
  generic format dispatch, imports, codec routing, streaming residency
  controls, VFS source mounts, semantic models, textures, and collisions.
- Provider has Arc-backed registry snapshots: no global registry mutex
  is held across a slow codec decode. ListFile codec retains bounded
  NEF8 body caching to reduce repeated large YTD inflation.

## Model/YDD migration

NewViso `newviso-binary-asset-bodies` defines an optional YDD V5
per-mesh inline PBR material payload, serialized as a bounded JSON
object in the string table with a 52-byte mesh header. The decoder
accepts legacy V2–V4 bodies, while the encoder preserves V4 for assets
without inline material and emits V5 only when inline material exists.
Regression verifies V5 full and selective-entry decode, V4 round trip,
material preservation and invalid-shape rejection.

The StarVault YDD format descriptor now advertises writable V5 and
readable V2–V5. Format modules derive NEF8 content-kind IDs from the
current `newviso-asset-interfaces` rather than private numbers.
`YSCD` is treated as the canonical `YSNCD` sound-cue dictionary
content identity.

## Build, ABI, tests

- `cargo check --offline -j 2 -p engine-assets-starvault -p newengine-codec-listfile`: PASS
- StarVault Rust unit tests: **38 pass**, 0 fail.
- ListFile unit tests with `STARVAULT_TEST_FORMATS_DIR` pointing to
  a staging directory containing 30 provider-declared DLLs:
  **26 pass**, 0 fail, 1 intentionally ignored full corpus test.
- NewViso binary resource tests: **15 pass**, 0 fail.
- NewViso semantic and host API unit tests: pass.
- `cargo check -p newviso-runtime --offline -j 2`: PASS.
- `cargo build --release --offline -j 2 -p engine-assets-starvault
  -p newengine-codec-listfile`: PASS.
- 30 first-party format modules built in Release.
- Remapped and stripped release rebuild of provider, codec and all
  30 format modules passed the native source-path audit
  `PluginsSrc/build_support/audit_release_binary.ps1`.
- Exact staged provider `starVault-assetManager-3.8.1-release.dll`
  loaded by NewViso; exact ListFile 0.1.1 and 30 formats registered.

## Cold startup and gameplay QA

**New binary isolated, 1,100-frame test**:
`NewViso/cache/profiles/live-KaylaNightCity-20261112-071006.json`.
`map_ready=true`, `gameplay_ready=true`, 428 steady gameplay frames,
zero failed stream assets, finished world and physics ticks.

**Old installed StarVault 3.8.0, matched 1,100-frame test**:
`NewViso/cache/profiles/live-KaylaNightCity-20261112-071320.json`.
`map_ready=false`, 0 gameplay frames, unfinished stream queue.
**No old-version gameplay FPS comparison is possible.**

**Final source-path-hardened staging test**:
`NewViso/cache/starvault-hardened-qa-20261112.out.log`.
`map_ready=true`, `gameplay_ready=true`, 459 steady frames,
0 failed resources and no outstanding stream queue.
45 steady GPU samples: frame span mean 10.9238ms, p95 16.6404ms,
p99 18.3821ms; scene-dependent, not a universal guaranteed FPS.

**Installed production directory after publication**:
`NewViso/cache/profiles/live-KaylaNightCity-20261112-072552.json`.
`map_ready=true`, `gameplay_ready=true`,
`benchmark_valid=true`, 372 steady samples,
world frame 422, physics fixed tick 1257,
zero failed resources and no outstanding stream queue.
37 steady GPU samples: frame-span mean 20.8136ms,
p95 31.3959ms, p99 34.4605ms. The difference between runs
warns against inferring a uniform improvement in gameplay FPS.

## Transaction and rollback

**Installed** as the only AssetManager and only ListFile codec:
- `pluginsRuntime/starVault-assetManager-3.8.1-release.dll`
  SHA256 `2978d962a8557cb7083bb86fd8b64a820fac4e57fc03787c1816858983b03af3`
- `pluginsRuntime/codecs/newengine-codec-listfile-0.1.1-release.dll`
  SHA256 `203d86dc5e5e93f951e9be10d16299088e56405b08db119c21fe1510088b3c77`
- `pluginsRuntime/formats/`: 30 inspected DLLs.

A candidate tree was assembled without modifying the live installation;
all 32 updated DLL hashes checked and all 30 format binary audits passed.
Live `pluginsRuntime` was renamed into
`NewViso/cache/starvault-runtime-backup-20261112` before the candidate
directory was renamed into place. The old DLLs do not coexist with the
new files in the provider discovery folder.

Transaction manifest (old/new SHA256 and state):
`NewViso/cache/starvault-install-20261112.json`.

To restore the old runtime after stopping `newviso.exe`:

```powershell
python NewViso/scripts/publish_staged_assetmanager.py rollback
```

This swaps the full runtime directory back and verifies the old
AssetManager SHA256. Do not delete the backup until the new release
has completed longer production soak and visual/physics collision
regression tests.

## Remaining performance QA

- Compare multiple cold and warm boots at identical settings, scene
  and GPU load; retain startup time, provider inflate/cache-hit time,
  memory and gameplay CPU/GPU p95/p99 distributions.
- Run sustained map streaming through all Night City sectors,
  test provider hot reload, collision and texture pixel fidelity,
  and verify V5 inline material against original authored assets.
- The release smoke proves the bus/vehicle, physics and startup
  at this one scene, not exhaustive coverage of an entire city.
