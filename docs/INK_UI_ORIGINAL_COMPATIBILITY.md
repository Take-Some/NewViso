# Cyberpunk 2077 Ink UI compatibility — NewViso

Status: **source inventory and neutral runtime infrastructure implemented; full visual and behavioral parity NOT achieved**.

This document separates source evidence from functionality that actually executes
in NewViso. No original native controller code was recovered from Ink assets.

## Local source inventory

Original installation: `C:\Games\Cyberpunk 2077`

Local extraction: `Projects/KaylaNightCity/.newviso/cache/ink_full_ui_original`

| Source resource | Extracted count |
|---|---:|
| `.inkwidget` | 1,701 |
| `.inkanim` | 734 |
| `.inkatlas` | 1,355 |
| `.inkstyle` | 208 |
| `.inkfontfamily` | 19 |
| `.inkhud` | 16 |
| `.inkmenu` | 2 |
| `.credits` | 13 |
| `.inkshapecollection` | 10 |
| `.charcustpreset` | 6 |
| `.ccstate` | 2 |
| `.inkfullscreencomposition` | 2 |
| `.inkenginesettings` | 1 |
| `.inkgamesettings` | 1 |
| `.inklayers` | 1 |
| `.inktypography` | 1 |
| **Total** | **4,072** |

Source `.inkwidget` declarations do **not** contain native/redscript implementations.
Their controller classes and serialized configuration can be indexed but cannot
be treated as runnable callbacks.

Two original `.inkmenu` documents declare **95 entries, 78 unique screen
routes and 43 scenario names**. The screen routing catalog is preserved in
`Projects/KaylaNightCity/ui/ink/original_menu_registry.json`.

The `inkHUD` resources describe **16 HUDs with 561 spawn entries**, including
visibility conditions, scene tiers, input/game contexts, anchor coordinates,
slot attachment, effect eligibility and widget resource references. These
records are in `Projects/KaylaNightCity/ui/ink/original_hud_registry.json`.

The initial fullscreen analysis covers **121 decoded widget libraries**,
exposing **317 distinct controller class names**. The remaining source libraries
are indexed by original depot path but are not all converted into runnable
NewViso surfaces. Complete per-resource details:
`Projects/KaylaNightCity/story/original_ink_ui_audit.json`.

## Implemented engine-level contracts

| Capability | Status |
|---|---|
| Native resource discovery and typed inventory | Implemented |
| Original menu registration and scenario identity | Implemented |
| Provider-neutral screen navigation with history | Implemented |
| Back, close, replace, push and overlay semantics | Implemented |
| Focus restoration, action and property event routing | Implemented; events published to `newviso.ui.ink.event` |
| Source context-based HUD eligibility | Implemented |
| Original HUD anchors/slot metadata retained | Implemented |
| Runtime registration of 78 source screen routes | Verified in live game |
| Existing main menu/settings functionality | Retained |
| Entire source Ink widget tree rasterized | **Not yet** |
| 317 source-specific controller implementations | **Not yet** |
| Source Ink full animation/effect engine parity | **Not yet** |
| All atlas/XBM/material/font assets rasterized | **Not yet** |
| All original localized strings / accessibility | **Not yet** |
| Inventory, journal, map, phone, scanner behavior | **Not yet** |
| Every HUD mode visually and interactively tested | **Not yet** |

This separation is intentional: registering a menu route is not equivalent to
implementing its interface or game-state controller.

## Architecture and files

The generic, provider-independent engine APIs are in:

- `NewViso/crates/newviso-ui-client/src/ink_navigation.rs`: validated original
  menu registry, screen stack, focus, state patches and input dispatch.
- `NewViso/crates/newviso-ui-client/src/ink_hud.rs`: source HUD eligibility,
  anchoring and slot rules.
- `NewViso/crates/newviso-runtime/src/application_main_menu.rs`: native
  source registration and menu/settings action integration.
- `NewViso/crates/newviso-runtime/src/application_platform/frame.rs`: in-game
  pause route integration.

Project tools and generated data:

- `tools/audit_original_ink_ui.py`
- `tools/import_original_ink_hud.py`
- `ui/ink/original_menu_registry.json`
- `ui/ink/original_hud_registry.json`
- `story/original_ink_ui_audit.json`

The UI providers (Egui, Aurelia) receive ordinary `engine.ui` surfaces. They
should **not** directly implement game semantics, source menu scenarios, inventory
mutations or save operations.

## Source resource gaps

Two registered source screens are absent from the extracted path index:

- `base/gameplay/gui/fullscreen/storage/storage.inkwidget`
- `base/gameplay/gui/fullscreen/main_menu/character_creation_persistant_elements.inkwidget`

Three unique HUD dependencies are also absent:

- `base/gameplay/gui/widgets/notifications/messagepopup.inkwidget`
- `base/gameplay/gui/widgets/hud_preview/hud_preview.inkwidget`
- `base/gameplay/gui/widgets/quests/quest_update.inkwidget`

These might be packed under unresolved hashes or represented through another
source mechanism. The importer reports them and the runtime refuses to mark
an absent source asset as renderable.

## Verification

Successful checks:

- `cargo test -p newviso-ui-client --lib` — 9 tests (with actual menu/HUD
  registry environment variables)
- `cargo test -p newviso-runtime --lib` — 233 passed, 1 ignored
- `cargo build -p newviso --release` — succeeded for the menu-router integration
- Live game: `registered original Ink menu graph screens=78 scenarios=43`.
  Screenshot: `Projects/KaylaNightCity/.newviso/verification/original_ink_registry_runtime.jpg`.

For the source-aware UI client tests, set:

```powershell
$env:NEWVISO_ORIGINAL_INK_MENU_REGISTRY_TEST_PATH = "C:\Users\Kayla\Documents\NorthStar\Projects\KaylaNightCity\ui\ink\original_menu_registry.json"
$env:NEWVISO_ORIGINAL_INK_HUD_REGISTRY_TEST_PATH = "C:\Users\Kayla\Documents\NorthStar\Projects\KaylaNightCity\ui\ink\original_hud_registry.json"
cargo test -p newviso-ui-client --lib
```

## Required next engineering stages

1. Convert each source widget library to typed `UiSurfaceNode` trees,
   including layout anchors, nine-slice, rich text, scroll areas, focus graph,
   property managers and controller-binding descriptors.
2. Bind actual atlas UVs, XBM material samplers, font families, video textures,
   localized strings and source effect parameters to both UI providers.
3. Implement the original Ink animation scheduler with callbacks, named event
   tracks and lifecycle events, not just sampled interpolator curves.
4. Provide explicit, testable controller adapters for gameplay features:
   save/load, inventory, cyberware, perks, crafting, map, journal, scanner,
   calls/messages and vehicle dashboards. Source controller names alone are
   insufficient.
5. Add UI screen-state tests using game snapshots plus in-game pixel
   comparisons and interactive keyboard/controller/mouse navigation tests.

**Do not mark overall parity complete until these stages pass in-game tests.**
