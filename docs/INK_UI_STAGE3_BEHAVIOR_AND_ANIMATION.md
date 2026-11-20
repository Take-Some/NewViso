# NewViso Ink UI — stage 3

Status: partial implementation; full REDengine Ink parity is not complete.

## Runtime changes

- Source `inkVirtualCompoundWidget` lists now render inventory and backpack
  rows with a bounded viewport, paging, selection and retained source geometry.
  Rows contain real `ItemsRuntime` names and quantities, not fabricated data.
  User actions are checked against the current inventory model. Equipment goes
  through `ItemsRuntime::equip` and requires active gameplay and ownership.
- `build_ink_runtime_surfaces.py` writes original child-index animation paths
  and source track bindings into preview metadata. For the original main menu,
  all 11 clip definitions and 199 interpolators are decoded. 22 of 74 track
  occurrences currently bind to drawable nodes; missing and dynamic nodes
  remain unsupported. The runtime now plays these original curves on the
  isolated source preview and caches its base widget JSON.
- `world_persistence.rs` offers an explicit Ink quicksave action using the
  existing validated and atomic save/checksum/backup machinery. Save requests
  are rejected before gameplay readiness or without configured persistence.
  The source save menu preview has a guarded button and emits
  `newviso.ui.ink.event` with a save_result. This is one project checkpoint,
  not named save/load slots.

## Evidence

- UI client: 19 Rust tests passed (including a test against the original extracted inventory surface).
- Runtime: 237 Rust tests passed, one ignored, including real 11-clip
  animation source and rendered-node property regression tests.
- Egui: two source surface DTO smoke tests passed for 18 preview screens.
- Python compiler: five tests passed.
- Active project currently uses `depth_stability_qa.scene.json` and
  `traffic_qa_hidden.json`; the manifest was left unchanged.

## Outstanding

Original native controller implementations (save-slot enumeration and load,
character creation, crafting, journal, phone, scanner, map, cyberware),
full Ink layout and controller-sized widgets, full 11-clip target binding,
effect/material/video/atlas pipelines, source font layout and reference
pixel comparison remain incomplete. Egui/Aurelia visual parity is unverified.

Do not describe this as a fully compatible Cyberpunk 2077 Ink port.
