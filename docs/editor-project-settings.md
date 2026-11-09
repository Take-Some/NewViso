# Native Project Settings and object actions

Launch from the NorthStar root:

```cmd
Scripts\Run\RunEditor.cmd WeaponLab release
Scripts\Run\RunEditor.cmd MadMaxWasteland release low_memory
```

The toolbar **Project Settings** button and **Runtime Settings** entry open the native settings panel. The manifest remains accessible as **Project Manifest**.

The panel follows `ProjectRuntimeSettings::schema()`: categories, search, typed fields, source labels, live/restart timing, undo/redo and revert. Invalid drafts stay visible and block save/apply/closing until corrected or reverted. The input shows the project override where present, otherwise the inherited value. A separate effective value identifies properties masked by a profile, user layer or launch override. **Use inherited value** removes an override rather than copying defaults into the project.

**Save Settings** writes only the project-owned runtime layer using compare-before-save and atomic replacement. Its physical target follows configured VFS priorities, including project assets overlays. Shared engine defaults and external mounts are not writable. Other layers are snapshots of the selected launch configuration; restart the editor after changing those files or choosing another profile.

**Apply Live** is explicit and does not save the file. It sends only changed live properties through `runtime.configure` and checks that their previous runtime values still match. Unrelated runtime edits are preserved. Window, persistence and startup command edits remain pending until restart. Removing object keys from project variables requires restart because runtime configuration uses recursive object merge. Project-defined variable consumers retain responsibility for their own contracts and refresh behavior.

Select an entity in the Scene Outliner, then right-click or press **...** for object actions. Properties selects the entity in the inspector. Duplicate, delete and add/remove component actions use the same validated scene transaction journal, undo/redo and save conflict checks. Required native components and linked-scene identity restrictions remain enforced. An obsolete menu cannot act against a newer scene revision.

The inspector exposes native transform, model reference, primitive material, collider, LOD and destructible settings. **Game Properties** adds a project-defined JSON object:

```json
{"properties":{"data":{"item_id":"crate.small","interactable":true,"weight":2}}}
```

The `properties.data` object is preserved in scene records and the scene provider's generic entity payload. Project code assigns its semantics. It does not automatically create inventory definitions, attach behavior, or implement game rules. Existing item definitions and script configuration can be edited through runtime variables.

Scene-object edits still apply on the next scene launch. This stage does not add live scene reconstruction, viewport picking/gizmos, Play/Stop, prefab instancing or visual scripting. Existing first-person controls, weapon aim and original assets are unchanged.
