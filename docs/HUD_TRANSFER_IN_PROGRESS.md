# Original Cyberpunk 2077 HUD transfer — 2026-11-12

Source: `C:\Games\Cyberpunk 2077`; project: `Projects/KaylaNightCity`.

The extracted original Ink registry contains 16 HUD documents and 561 spawn entries. The native `newviso-ui-client/src/ink_hud.rs` importer now preserves anchor points, spawn-before-slots, separate-window and spawn-mode fields in addition to existing context/scene gating, slot layouts, margins and effect flags.

**Not completed:** original widget rasters, full source atlas/XBM binding, Ink shader and animation execution, native/redscript game-state controllers, live provider integration and visual parity testing. The existing screen-space, unlit HUD remains in use; do not call this a complete 1:1 port.

`NewViso/scripts/build_ink_hud_sources.py` generated `NewViso/cache/hud_source_transfer/source_registry_snapshot.json`: 16 profiles, 561 original spawn entries, 98 unique widget dependencies. This is a manifest, not decoded/loaded visuals. The updated native crate passed 19 unit tests (`cargo test -p newviso-ui-client --lib --locked`). The game project and asset archives were not overwritten.
