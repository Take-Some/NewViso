# Night City YTD multi-entry startup bottleneck — 2026-11-11

## Confirmed observation

Live diagnostic run `cache/profiles/live-KaylaNightCity-20261111-130114.stderr.log`
showed all eight source/decode workers loading different selectors from:

`textures/world/nightcity/cp77_newviso_multilayer_maps.ytd`.

The active set changed between trace frames 120, 240, 360, and 480
(e.g. `@concrete_wear`, `@cotton_shirt_clean_01_30_r`,
`@denim_clean_01_30_r`, `@dust_light_01_50_n`, `@edgewear_01`).
The workers were not proven deadlocked; they were progressing in waves.

The actual file at
`Projects/KaylaNightCity/assets/textures/world/nightcity/cp77_newviso_multilayer_maps.ytd`
is 191,444,672 bytes on disk, with an NEF8 compressed body length of
191,444,608 bytes and **uncompressed body length 570,627,572 bytes**.
The list-file header contains a non-zero raw body hash.

The `newengine-codec-listfile` plugin already has a content-identity
single-flight decompression cache in `src/nef8.rs`, but its previous
256 MiB admission ceiling excluded this 570.6 MB body. It therefore
could not retain the decompressed body between successive groups of
`.ytd@entry` requests, causing redundant work.

## Source patch

`PluginsSrc/AssetManager/codecs/newengine-codec-listfile/src/nef8.rs`

- Increased aggregate NEF8 cached-body retention budget from 256 MiB
  to **768 MiB**, enough for the known Night City dictionary.
- Kept the eight-entry and LRU eviction limits, content-hash identity,
  and single-flight synchronization; no unbounded cache growth.
- Added a unit test for the exact known dictionary size and rejection
  of an entry greater than the configured budget.
- `rustfmt --check --edition 2021` passed.

## Current NewViso pipeline and build/deployment boundary

The authoritative engine is **NorthStar/NewViso**, not Legacy NewEngine.
The current data flow is:

1. `newviso-resource-runtime::AssetClientSource` single-flight-caches raw
   YTD source bytes through the active `engine.assets` service. Its 256 MiB
   raw-source budget covers the approximately 191 MB source YTD.
2. `newviso-rsc7-import::SemanticTextureDecoder` resolves the texture
   dictionary from the current asset type registry, requests
   `assets.textures.entry_runtime_v1` through
   `newviso-assets-client::AssetClient`, and materializes a native
   `TextureResource` from the returned NTRT v2 packet.
3. The installed StarVault/AssetManager provider owns format decoding.
   No retired engine host is used by NewViso at runtime.

The provider source repository currently contains stale Cargo paths into
`NewEngine/neocore2`. **Do not restore or adopt Legacy NewEngine** as a
solution; the temporary Legacy checkout used in an unsuccessful experiment
has been removed. The provider must be built against supported *current*
contracts rather than resurrecting an obsolete engine dependency graph.

The bounded NEF8 body-cache change is present in the current provider
**source**, but **has not been deployed** to
`pluginsRuntime/codecs/newengine-codec-listfile-0.1.0-release.dll`.
Do not claim a runtime speedup until that provider is built, deployed and
tested using compatible current contracts.

## Validation on current NewViso

1. Keep the existing `newviso-resource-runtime` /
   `newviso-rsc7-import` / `newviso-assets-client` architecture.
2. Identify the active asset provider's current supported SDK and replace
   obsolete provider build references without restoring Legacy NewEngine.
3. Build and test the provider, deploy the codec atomically with matching
   manifest and recoverable backup.
4. Run the same Night City autostart benchmark and inspect
   `newviso.perf.startup` and `newviso.perf.startup.inflight`.
5. Confirm that required collision and texture interests become resident
   and the game reports `initial map residency ready`.
6. Only then collect at least 500 gameplay frames and compare p95/p99
   CPU/GPU times under identical conditions.

## Memory tradeoff

The large decompressed dictionary consumes approximately 544 MiB of
process memory while resident, in addition to source bytes, decoded
entries, and transient inflate/parse allocations. This is a bounded
CPU-side cache, not a VRAM allocation. Monitor peak commit/RSS and
eviction churn during real Night City runs. If memory pressure is too
high, the longer-term solution is independently addressable texture
entries or chunk-indexed NEF8 bodies rather than repeated full-dictionary
decompression.
