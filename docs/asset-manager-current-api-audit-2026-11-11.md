# Current asset manager API audit and migration checkpoint
Date: 2026-11-11

## Authoritative runtime state

- Engine: NorthStar / NewViso, Rust/Vulkan.
- Host ABI: `newviso-compat-abi::provider::ServiceV1` with
  `newviso-host` `engine.assets` service gateway.
- Current client: `crates/newviso-assets-client`.
- Current selected provider ID: `engine.assets.starvault`, version **3.8.0**.
- Installed provider: `pluginsRuntime/starVault-assetManager-3.8.0-release.dll`.
- Registered concrete service: `asset_manager.api`, advertised contract
  `newengine.assets-api.v1`, 54 declared methods.
- Separate host-owned type resolver: `asset.types.api` alias `engine.assets.types`.
- Runtime codec: `pluginsRuntime/codecs/newengine-codec-listfile-0.1.0-release.dll`.
- Format modules are registered from `pluginsRuntime/formats`; no format list is hard-coded
  in NewViso's asset client.

The current default provider selection at
`crates/newviso-config/src/lib.rs` is `engine.assets.starvault`.
The current service method set includes VFS mounting, VFS list, text,
raw bytes, semantic codec decode, source/status/import/streaming APIs.
Simply removing this provider or changing its ID breaks scene loads.

## Changes implemented in current NewViso

1. `crates/newviso-service-api/src/assets.rs`: canonical engine-owned asset
   gateway identifiers, required runtime asset method set and a provider-description
   validator independent of vendor/DLL identity. Validates method presence instead
   of matching retired SDK identity.
2. `crates/newviso-service-api/src/lib.rs`: exposes the canonical asset gateway.
3. `crates/newviso-assets-client/Cargo.toml` / `src/lib.rs`: imports
   `newviso-service-api` constants and adds `verify_provider_contract()`,
   replacing local duplicates of the gateway/method strings.
4. `crates/newviso-host/src/lib.rs`: added `service_description` accessor
   that clones the selected service before invoking its ABI `describe()` method,
   without holding the host registry lock.
5. `crates/newviso-runtime/src/bootstrap.rs`: validates the selected backend
   contract before VFS/project mounts and logs its concrete service ID, declared
   contract and method count. ABI mismatches now cause explicit startup failures.

## Verification

- Validated the source-owned currently installed StarVault service metadata:
  54 methods; required VFS/text/raw/decode methods all declared.
- `cargo test -p newviso-service-api -p newviso-assets-client --lib --offline -j 2`:
  29 tests passed, 0 failed.
- `cargo check -p newviso-runtime --offline -j 2`: succeeded.
- `cargo build -p newviso --release --offline -j 2`: succeeded.
- Actual smoke report:
  `cache/profiles/live-KaylaNightCity-20261111-161322.json`.
  Release executable started the selected provider and logged:
  `asset gateway validated service='asset_manager.api' contract='newengine.assets-api.v1' methods=54`.
  240 renderer frames, 45 resident, 242 queued, 8 loading, 0 failures.
  Game simulation frames: 0; map_ready=false. This is NOT a valid FPS benchmark.

## Why the provider DLL was NOT replaced

The AssetManager provider source at `PluginsSrc/AssetManager` still has
Cargo path dependencies to absent `NewEngine/neocore2` crates, including
`newengine-asset-format-nef8`, `newengine-assets-api`,
`newengine-texture-container`, and the retired plugin kit.
Attempted offline Release build failed at manifest resolution, before Rust
compilation. It would be incorrect and unsafe to copy an old or incompletely
built DLL into the runtime under a newer name.

Active source-only improvements in this repository:
- `newengine-AssetManager/src/module/codecs.rs`: release codec registry
  Mutex before invoking expensive `codec.decode()`;
- `codecs/newengine-codec-listfile/src/nef8.rs`: bounded 768 MiB LRU
  retention for a 570,627,572-byte inflated Night City YTD dictionary.

These patches have NOT been deployed. They are not active in the currently
installed provider/codec DLLs. Do NOT restore Legacy NewEngine SDK.

Installed binary hashes observed before attempted replacement:
- AssetManager 3.8.0 SHA-256:
  `8a537d45dd809baecfb16d3158364cf3ab0ee2f4dfdecd001ad91a6f0821b9bb`.
- Listfile codec 0.1.0 SHA-256:
  `85dfbc4214a9a3f5d9fc00023179f355388c4eaa79a576ec4850179d72758b08`.

## Remaining deployment gate

Migrate the existing provider (its VFS, codec loader, imports and streaming
methods) onto the current `newviso-compat-abi` and
`newviso-service-api` contracts with equivalent semantics. No guessed
one-to-one crate substitutions and no incomplete replacement stub.

Then: build and test the full provider + codec → validate ABI signatures and
metadata → compare required method set, VFS precedence and representative
NTRT/NRMT/geometry/collision outputs → atomically install the versioned
Release DLLs through the canonical PluginsSrc/Scripts build pipeline with
hash-verified stamp and rollback → cold-start the entire KaylaNightCity map
and measure performance only after actual gameplay begins.

**Current completion status:** new engine-owned API client and contract
preflight are implemented, Release built and verified. Backend replacement
is BLOCKED by missing migrated provider source dependencies. Runtime DLLs
were NOT replaced. No gameplay FPS or CPU/GPU p95/p99 is claimed.


## Release publication gate (same-day follow-up)

The requested full StarVault 3.8.x replacement was attempted through the
canonical `PluginsSrc/AssetManager/buildRelease.bat` entry point.
**Build exit code: 3**. `cargo metadata` cannot resolve the missing
`NewEngine/neocore2/crates/newengine-asset-format-nef8/Cargo.toml`.
The currently installed provider was NOT replaced; no v3.8.1 DLL
was compiled and none is claimed.

Production release safety tools were created and built using current NewViso:

- `crates/newviso-provider-runtime/src/bin/asset_provider_probe.rs`:
  standalone current-ABI DLL probe that validates provider ID, actual embedded
  version, root signature, descriptor v2 and lifecycle metadata. Release build passed.
- `scripts/publish_asset_manager.py`: atomic release gate that verifies
  the actual binary signature, *all* previously declared service methods
  and capability IDs/versions, payload/return contracts, candidate SHA-256,
  backup, installation and runtime smoke, with rollback on failure.
- `scripts/tests/test_publish_asset_manager.py`: six automated publication
  compatibility tests passed.
- `cargo test -p newviso-provider-runtime --lib --offline -j 2`: 1 passed.
- `python scripts/publish_asset_manager.py` with no candidate confirms the
  installed release has 54 methods, nine capability declarations and SHA-256
  `8a537d45dd809baecfb16d3158364cf3ab0ee2f4dfdecd001ad91a6f0821b9bb`.
- A fake 3.8.1 candidate (byte-copy of 3.8.0 under a new filename) was
  rejected by the ABI signature probe; expected exit code 2.
- The fake candidate was deleted. No file in `pluginsRuntime` was replaced.

The original StarVault source depends heavily on non-present
`newengine-assets` and `newengine-texture-container` APIs, in addition
to other missing local Rust crates. The minimal independent
`PluginsSrc/formats/newengine-assets-api` descriptor package is not a
drop-in replacement for the full old asset runtime and must not be
assumed to be one. No Legacy SDK checkout was restored.

Publication remains blocked until a real new provider is compiled
against `newviso-compat-abi` and the current service/VFS/semantic
contracts, with full codec and collision parity tests. The publisher
deliberately refuses a simple rename or a contract-incomplete DLL.
