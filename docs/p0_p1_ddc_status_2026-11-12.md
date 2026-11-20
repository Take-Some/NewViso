# NewViso P0/P1 checkpoint — 12 November 2026

Current RenderGraph compiler already has DAG, culling, transient alias and lifetime analyses; Vulkan backend barriers, real alias allocations and queue scheduling remain future work. 59 render-api tests passed. The current `engine.assets` gateway and provider-neutral asset interfaces remain active; 33 asset-interface tests passed.

**P0:** StarVault 3.8.1 is still source-only and blocked by missing `newengine-assets`, plugin ABI, maps runtime and model-domain code. Its currently deployed DLL remains 3.8.0. A Cargo dependency alias from old `newengine-assets-api` to the tested NewViso `newviso-asset-interfaces` package was added; no new StarVault DLL was built or installed.

**P1 implemented:** `crates/newviso-derived-cache` and optional integration in `crates/newviso-rsc7-import/src/texture_ddc.rs` cache provider-produced NTRT v2 packets by a content-derived BLAKE3 key (source bytes, path, texture entry, output kind, decoder revision, provider revision). Hits are checksum verified and revalidated by the actual semantic decoder; any failure falls through to `engine.assets`. Records use atomic writes, corruption recovery and maximums of 256 MiB per entry and 2 GiB on disk, distinct from memory/VRAM budgets. Startup and collision readiness were not modified. Initial 4 DDC tests, 23 semantic tests and `cargo check -p newviso-runtime` passed.

The cache is OFF by default. Enable intentionally by setting BOTH `NEWVISO_DDC_DIR` and `NEWVISO_DDC_PROVIDER_REVISION`, e.g., `starvault-3.8.0-listfile-0.1.0`. Provider revision must change when the backend semantics change. `NEWVISO_PERF_TRACE=1` emits optional cache-hit diagnostics. A real gameplay performance gain has not been established.

**Remaining:** release rebuild, controlled cold/warm cache comparison, full StarVault rebuild and ABI/format/collision parity, `map_ready=true` and >=1000 gameplay samples before FPS claims. P2 should extend the existing graph rather than create a duplicate; P3/P4 remain planned.
