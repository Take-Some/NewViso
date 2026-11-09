# Native editor viewport

Run from the NorthStar workspace:

    Scripts\Run\RunEditor.cmd MadMaxWasteland release
    Scripts\Run\RunEditor.cmd WeaponLab release

The viewport is active with --editor and the project's entry scene open. It has a separate camera, suppresses game scripts and pauses scene physics. Looking around does not modify the saved game camera. Ordinary game input and first-person weapon aiming are unchanged.

| Input | Action |
|---|---|
| Left click a surface | Select the nearest object; empty space clears selection |
| W / E / R | Move / rotate / scale |
| Space | Cycle the three transform tools |
| Drag red / green / blue handle | X / Y / Z axis |
| Drag a small plane square | Move in two axes |
| Drag the white center | Free movement in the view plane / uniform scale |
| World / Local button | Translation and rotation basis; scale uses local axes |
| Snap button / Ctrl while dragging | Persistent / temporary snapping |
| Step buttons | Cycle movement, angular and scale increments |
| F | Frame the selected object's bounds |
| RMB + mouse, WASD | Look and fly |
| RMB + Q / E | Down / up |
| Shift while flying | Four times faster |
| Wheel while holding RMB | Change flight speed |
| MMB + mouse | Pan |
| Alt + LMB | Orbit around the selection, or the current view target |
| Wheel | Dolly forward/backward |
| Ctrl+Z / Ctrl+Y / Ctrl+Shift+Z | Undo / redo |
| Ctrl+S | Save the scene |
| Escape during a drag | Restore the original transform without adding history |
| Stationary RMB click / outliner ... | Object context menu |

The inspector includes tool, coordinate-space, snap-step, focus and preview-light buttons. Preview lighting is editor-only and is not saved into the scene. Scene light restores the original environment lighting. Panels and popup geometry block viewport picking, including resized docks on the installed action-only UI provider.

A drag previews directly in the native scene without serializing the whole map or adding history per mouse movement. Native transforms follow every changed input sample; retained inspector updates are limited to about 15 Hz during a drag, and unchanged UI surfaces are not republished. Release commits all transform fields as one validated transaction. Save and competing property edits are blocked during a drag. Inspector values, undo/redo and Revert Saved synchronize back to the native object. Session identities survive array-index shifts, deletion, undo/redo, saves and browsing other assets.

Picking uses sorted world bounds as a broad phase, oriented primitive boxes and resident imported triangles for the narrow phase. Heightfields use their physical triangles. Nonvisual/unloaded actors have bounds proxies. Gizmos keep a stable screen size and highlight hovered/active handles. Their solid lines use the engine's existing UI primitive texture; no model, material or bitmap asset is generated.

Current scope is one selected object at a time. Existing mesh/material/component replacement needs project reload. Newly created cubes, model actors and nonvisual proxies preview live; deletion is reversible editor visibility. Baked heightfield grids are selectable and can be framed, but their mesh gizmos are disabled: their world-space grid must be edited through the terrain source. The inspector reports this limitation when selected.

Implementation ownership:

- newviso-editor-viewport: pure projection, navigation, gizmo geometry and drag math;
- newviso-scene-document: validated transactions, stable session identities and bounded undo history;
- newviso-content-manager: selection, property drafts, toolbar and authoring transactions;
- newviso-scene: opaque source-to-runtime bindings, bounds and precise picking;
- newviso-runtime: editor-only input coordination, live synchronization and neutral UI overlay composition.
