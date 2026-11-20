# Original Cyberpunk HUD resource transfer — 2026-11-12

Scope: source data from `C:\Games\Cyberpunk 2077` into `Projects/KaylaNightCity`. Existing production HUD is retained. **This is not a complete 1:1 visual and behavioral port.**

## Verified new source conversion
- 98 original HUD widget paths from 16 original Ink HUD documents. 95 extracted CR2W `.inkwidget` resources converted with locally installed WolvenKitCLI 10 via .NET 10; 3 source files unavailable in extracted set: `hud_preview.inkwidget`, `messagepopup.inkwidget`, `quest_update.inkwidget`.
- All 95 decoded widgets compiled into `ui/ink/generated/hud/*.surface.json`: 4,132 original widget nodes, 1,923 emitted visible UI components. Unsupported runtime-controlled sizing is flagged as approximate, not invented.
- All 67 referenced HUD `.inkanim` libraries decoded and exported: **506 clips, 4,625 tracks, 18,626 interpolators, 167 events**. **69** HUD surfaces refer to animation libraries; **997** track routes bound to visible targets, **3,828** routes remain unbound (nested/dynamic controller targets).
- Found 76 original HUD Ink atlas dependencies; 75 `.inkatlas` source records decoded: **6,035 original atlas parts, 168 unique XBM references**. Missing atlas: `base/gameplay/gui/fullscreen/placeholder_chardev/chardev_placeholder_atlas.inkatlas`. Every decoded atlas now has a collision-safe, source-path-hashed JSON name under `ui/ink/generated/hud/atlas_sources/source_items`.
- 104 exact source PNG sprite crops bound across 15 HUD preview surfaces. **964** additional HUD atlas references lack extracted pixel sources, and 18 have invalid or unset atlas-part metadata. No unrelated sprites are substituted.

## Active game integration
- The existing production UI still uses `scripts/story_hud.ysc` and native unlit screen-space Vulkan rendering.
- The actual source `hud_phone_animations.inkanim` `widgetIntro` root opacity track was exported to `scripts/generated/ink_phone_intro_source.ysc` (nine original key segments, duration about 0.34 seconds). `story_main.ysc` starts it on a genuine phone-opening transition, and the gameplay HUD applies the authored alpha to 17 project-owned phone-panel primitives. The original **curve** is real; the panel layout remains NewViso's, not the original REDengine phone controller or source raster.
- Existing bounded Scan/Glitch/Blackwall screen-space fallback treatments are still not original shader implementations. The actual REDengine Ink effect materials and Blackwall effects are not imported.

## Tests / outputs
- WolvenKit HUD widgets 95/95 selected originals decoded without converter errors. Runtime surface compiler 95/95. HUD animations 67/67. HUD atlas metadata 75/75 available originals.
- `tools/test_ink_phone_source_curve.cjs`: PASS (source transparency curve and 17 gameplay primitives).
- `tools/test_story_hud_ink_pixels.cjs`: PASS (event-only effects, no cutscene overlay).
- `tools/test_hud_ink_transition.cjs`: PASS (stable idle UI).
- `node tools/bundle_ysc.cjs`: PASS, regenerated `scripts/story_bundle.ysc`. Source font/scene guards passed.
- Native `cargo test -p newviso-runtime --lib --locked`: 253 passed, 1 ignored. Earlier `cargo build --release -p newviso --locked` completed; this stage changes source/project scripts and generated assets, not Rust engine code.
- Native verified capture after the updated gameplay bundle: PID 7496, foreground HWND ownership confirmed, HUD font atlas uploaded, client 1920x1080, 546 sampled colors, 0.0 near-white ratio. The screenshot is `.newviso/verification/hud_reference/nightcity_verified_hud.png`. **This is a real baseline Vulkan window capture, not an animated-frame verification or a same-state Cyberpunk 2077 comparison.**

## Next critical work
1. Extract and convert the 168 original XBM dependencies and bind real atlas parts/nine-slice/masks (not merely CR2W metadata). One pilot search in the `basegame_4_gamedata.archive` index for `atlas_phone.xbm` found no match; this does not prove the file is absent from other game archives.
2. Implement source Ink widget render transforms, hierarchical clipping and runtime-generated sizes for the 3,828 unbound tracks.
3. Integrate independent native HUD-gameplay controllers, source clip triggers, audio/video and one-shot event dispatch.
4. Implement original-equivalent Vulkan material/shader pipeline for Glitch/Blackwall; currently `UiPaintCommand::SurfaceEffect` remains deferred.
5. Perform identical-scene, identical-resolution screenshot and per-frame comparisons before claiming parity or replacing the stable HUD.

## Reproducible scripts and manifests
- `NewViso/scripts/decode_original_hud_widgets.py`
- `NewViso/scripts/compile_original_hud_surfaces.py`
- `NewViso/scripts/decode_original_hud_animations.py`
- `NewViso/scripts/export_original_hud_animations.py`
- `NewViso/scripts/bind_original_hud_animations.py`
- `NewViso/scripts/bind_original_hud_atlas_crops.py`
- `NewViso/scripts/decode_original_hud_atlases.py`
- `NewViso/scripts/export_ink_phone_intro_curve.py`
- `Projects/KaylaNightCity/ui/ink/generated/hud/` manifests for surfaces, animations, bindings, atlases, PNG crops.

Do not call this full shader/functional fidelity. Original Cyberpunk installation was not overwritten.
