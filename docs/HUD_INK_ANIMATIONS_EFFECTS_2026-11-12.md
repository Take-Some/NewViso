# Ink UI animations and effects — 2026-11-12

**Updated source HUD import:** `docs/HUD_SOURCE_CR2W_PORT_2026-11-12.md` documents 95 decoded HUD widget libraries, 67 source HUD animation libraries, 75 original atlas metadata resources, and one source-driven gameplay phone intro curve. Full pixel/effect/controller parity remains incomplete.

Game source: C:\Games\Cyberpunk 2077. Target: NorthStar/NewViso, KaylaNightCity.

## Implemented
- 97/97 locally decoded original .inkanim JSON exported: **764 clips, 7,128 tracks, 26,197 interpolators, 118 events**.
- Files: `ui/ink/generated/animations/`, index: `source_animation_manifest.json`. Source depot provenance preserved.
- Original animations bound by exact depot identity to **12** generated Ink preview screens; **1** already had bindings, **4** lacked a matching animation file, **1** had no target nodes. Index: `ui/ink/generated/source_animation_bindings.json`.
- Rust Ink interpolator interpreter extended to 15 source kinds. HDRColor interpolates UI RGBA without resetting existing alpha. Additional pivot/anchor/shear/text values retained as typed semantic node props.
- Native source timeline visual events: SetVisibility, ChangeState and ChangeTexturePart processed by their original startTime. Unresolved atlas sprite switches are metadata-only, not fake graphics.
- Original intro end pose is held after the clip ends to prevent a HUD reset/flicker when no loop is present. Existing stable unlit compositing remains in place.

## Not yet complete
Original Ink Glitch and Blackwall GPU shader parity, per-node material effects, dynamic atlas texture part raster binding, native controllers, video/sound playback and one-shot event dispatch. The 134 source inkAnim Effect interpolators are transferred as temporal data, **not** executed as original shaders. 734 original .inkanim resources were inventoried earlier; this batch covers the 97 already decoded libraries.

The production `scripts/story_hud.ysc` was not replaced by incomplete previews. No 1:1 visual parity or live Vulkan pixel comparison is claimed.

## Validation
- Python batch converter: 97 passed.
- Generated preview animation bindings: 12 new, all 12 target IDs and asset paths validated.
- `cargo test -p newviso-runtime --lib --locked`: 253 passed, 1 ignored.
- `cargo check -p newviso-runtime --locked`: passed.
- No Release binary built/deployed or gameplay-level effects QA completed in this work.
