# NewViso Ink UI stage 2 — source widget rendering and live game state

Date: 2026-11-11

**Status: source-driven preview and partial gameplay adapters.**
Neither original Ink pixel fidelity nor all 317 game-specific controllers are
complete. Source resources are read only from the user's installed Cyberpunk
2077 game and remain in their local project assets.

## Implemented

### CR2W widget-to-surface compilation

`NewViso/scripts/build_ink_runtime_surfaces.py` imports original serialized
`.inkwidget` files through the existing CR2W handle resolver and creates
standard provider-independent `engine.ui` surfaces.

- Source widget names, handle identifiers, widget classes and source-space
  geometry are retained in metadata.
- Fixed-size anchors, margins, translation, scale, visibility and tint are
  mapped into a 1600 × 900 frontend canvas.
- Original typography is resolved through the project's source
  `inkfontfamily` metadata and existing `font://cyberpunk/` font assets.
- Layout dimensions supplied by original native controllers at runtime are
  explicitly flagged and **not stretched to full-screen**.
- Original Ink layout classes not supported by NewViso remain recorded in
  the compiler's diagnostics.
- Interactive controller callbacks are not fabricated.

Current preview: **18 screens**, **1,065 emitted primitives**, **182 elements
with source font references**. The `new_game_menu` is currently a
dynamic-only root and emits zero primitives; it requires a real
`NewGameMenuGameController` equivalent before it can be shown.

Generated files are under
`Projects/KaylaNightCity/ui/ink/generated/*.surface.json`.

### Original sprites

`NewViso/scripts/export_ink_atlas_parts.ps1` uses source UV rectangles and
WolvenKit-exported original textures to crop each atlas sprite, preserving
color and alpha. `NewViso/scripts/bind_ink_atlas_sources.py` resolves source
Ink atlas paths and part names onto project-local relative PNG references,
never substituting unrelated graphics.

| Source Ink atlas | Cropped assets |
|---|---:|
| `menu_background` (2 resolution slots) | 50 |
| `atlas_shapes_sync` | 136 |
| `atlas_common` | 160 |
| `new_hub_layout` | 59 |
| **Total** | **405** |

The generated screens currently reference **399 original sprite instances**;
other materials, textures, UV inputs, video layers and effects are outstanding.

### Runtime game-state controller adapters

`NewViso/crates/newviso-ui-client/src/ink_game_state.rs` translates actual
`ItemsRuntime` data (definitions, categories, quantities, equipment and
revision) into a source-oriented inventory/backpack view model. It also
exposes the existing vehicle runtime snapshot.

`NewViso/crates/newviso-runtime/src/application_snapshot.rs` provides the
derived data at `runtime_state.ink_ui` and `script_frame_state.ink_ui`.

The view model explicitly identifies unimplemented crafting recipe authority,
world-map mappin controller, quest journal, phone messaging and scanner logic
instead of inventing data or enabling unsupported mutations.

### Independent preview mode

The project runs normally without any new preview UI. Developers can
activate an isolated original source surface while the menu is open:

```powershell
cd C:\Users\Kayla\Documents\NorthStar\Projects\KaylaNightCity
$env:NEWVISO_INK_PREVIEW_SCREEN = "singleplayer_menu"
.\RunInkUI.cmd
```

The runtime accepts **only** alphanumeric and underscore preview names, loads
`ui/ink/generated/<name>.surface.json`, and publishes it through
`engine.ui` before input dispatch. The source surface is deliberately not
used as the production main menu until its missing controller/renderer
features are complete.

An actual live preview was launched and captured. The first version exposed
a geometry regression: unknown 0×0 image sizes became huge sprites. It was
corrected and the preview captured again:

`Projects/KaylaNightCity/.newviso/verification/ink_source_singleplayer_postgate.jpg`

The preview still has missing/inaccurate Ink visuals; it is a proof of actual
asset-driven rendering, **not** reference visual parity.

Updated Release executable was compiled successfully and staged at:

`Projects/KaylaNightCity/.newviso/verification/newviso_ink_ui.exe`

This avoids modifying a potentially running engine executable.

## Verification

- `cargo test -p newviso-ui-client --lib`: **13 passed**.
- `cargo test -p newviso-runtime --lib`: **233 passed, 1 ignored**.
- Egui `surface_json_smoke` with
  `NEWVISO_INK_SURFACES_TEST_DIR`: **2 passed**; all 18 typed
  surfaces validated.
- `python NewViso/scripts/tests/test_build_ink_runtime_surfaces.py`:
  **5 passed**, including zero-size render regression.
- Staged Release build completed successfully.
- Live source preview mounted with 117 drawable source nodes; no runtime
  `ERROR` messages were observed in the tested launch.

## Still required to reach parity

1. **Ink layout fidelity:** container flex constraints, virtualized lists,
   auto-sizing/text fitting, scrolling, masks, clipping and nine-slice.
2. **Graphics:** all source atlases, XBM/material shaders, glitches,
   authored effects, layered blending and video playback.
3. **Animation:** full source clip graph, named callbacks, transitions,
   in-world widget events, event-target routing and timing validation.
4. **Controller adapters:** original menu flows, save/load slots, character
   creation and difficulty, inventory/equipment actions, map/mappins, quest
   journal, phone and contacts, scanning, crafting, perks, cyberware,
   world interactions, vehicle dashboards.
5. **Game-data authority:** all UI decisions must consult NewViso engine
   state/services. Source widget metadata alone does not implement behavior.
6. **Visual regression:** reference game captures at matching resolutions,
   contrast/typography/animation comparisons, input navigation and
   transitions tested in the running engine.
7. **Multi-provider support:** identical UI contract results in both Egui
   and Aurelia (the source preview is currently verified with Egui).

**Full UI/behavioral parity: NOT VERIFIED, NOT COMPLETE.**
