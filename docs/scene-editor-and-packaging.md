# Scene authoring and game packages

Scene authoring reuses the native Content Manager panels and the deployed UI provider, with interactive picking, navigation and transform gizmos. See [editor-viewport.md](editor-viewport.md) for controls and live-edit behavior. Visual scripting, prefab editing and full live resource/physics reconstruction remain separate tools.

## Open the editor

From the NorthStar workspace:

```cmd
Scripts\Run\RunEditor.cmd MadMaxWasteland release
Scripts\Run\RunEditor.cmd WeaponLab dev
```

The equivalent engine flag is `--editor`. This requests the UI backend, opens the project scene, suppresses project scripts and startup commands, disables world persistence writes, and enables the dedicated editor viewport camera. Ordinary game launches keep their previous behavior. No first-person aiming code is changed.

Project shows entry files/capabilities and a Scene Outliner. The outliner has name search and pages of 100 entities, so large maps do not publish thousands of UI controls per frame. Details becomes a Scene Inspector. Values are typed: vectors/colors use JSON arrays, numbers/booleans use JSON literals, names/asset references use plain text. Invalid drafts remain visible and cannot be saved.

You can edit registered properties, create a cube/camera/empty entity, duplicate ordinary entities, add/remove components, and undo/redo. Component metadata restricts applicable entity kinds and primitive types: camera/collider controls are not offered on incompatible objects, and imported model materials remain asset-owned. Required camera/mesh components and the last runtime camera/mesh cannot be removed. Removing entities from parent-linked/portal scenes is rejected until identity remapping is implemented. Explicit provider handles are not duplicated. Additional root/entity/component fields survive editing and serialization.

Transforms, including inspector edits and undo/redo, update the active editor scene immediately. New cubes/model actors and entity deletion also have a live editor preview. Existing mesh/material/component replacement and baked heightfields still require project reload. Save writes the authored project scene. Script files retain their existing save/reload workflow. Saving checks the original bytes against disk, writes and syncs a sibling temporary file, and atomically replaces the destination. An external edit causes a conflict instead of being overwritten. Selecting another file with unsaved valid changes is rejected.

Scene files under `scenes/` can be up to 64 MiB; general text previews retain the 2 MiB limit. Undo stores changed entity spans rather than copies of an entire map. History is bounded by 64 transactions and 16 MiB. A transaction larger than that memory budget cannot be retained in history.

## Shared scene contract

`newviso-scene-document` depends only on serde/serde_json. It owns:

- `SceneTypeRegistry`, `ComponentDescriptor`, `PropertyDescriptor`, and `EntityTypeDescriptor`;
- built-in metadata in `src/assets/scene-types.json`;
- `SceneDocument`, atomic batches of `SceneEdit`, optimistic document revisions and bounded undo/redo;
- common scene validation and authoring-file persistence.

The native scene loader, native inspector, and offline tool use this contract. Source-format decoding and renderer/provider APIs do not enter the registry. `SceneTypeRegistry::register` validates and atomically adds extension contracts; duplicate type/component identities are rejected. Built-in editor sessions currently use the built-in registry. Automatic loading of plugin-provided editor descriptors is a later integration point.

Validation covers the registered field types/ranges and basic native scene requirements. It is not a proof that every referenced asset exists or that an arbitrary plugin's additional fields are meaningful. Extra fields are preserved. The native scene/provider remains responsible for materialization, relationships and source assets.

## Offline commands

Build `newviso-project-tool` from NewViso, then:

```cmd
target\release\newviso-project-tool.exe types
target\release\newviso-project-tool.exe scene-validate --base-dir . --project ..\Projects\MadMaxWasteland
target\release\newviso-project-tool.exe scene-inspect --base-dir . --project ..\Projects\WeaponLab --entity 1
target\release\newviso-project-tool.exe scene-edit --base-dir . --project ..\Projects\WeaponLab --edits edits.json
```

`scene-inspect` returns the file's BLAKE3 revision, entity list, selected original record and typed property metadata. A scene edit request uses the inspected revision:

```json
{
  "expected_blake3": "copy the inspected blake3 here",
  "edits": [
    {"op":"set","entity":1,"path":"/transform/position","value":[0,1,-8]},
    {"op":"add_component","entity":1,"component":"lod"}
  ]
}
```

Other operations: `add` (`type_id`), `duplicate`/`remove` (`entity`), `remove_component` (`entity`, `component`). Every operation in a batch commits together. Failed validation and stale file revisions leave disk untouched. Editing shared fallback scenes is rejected; create a project-owned overlay first. `--scene` selects another logical scene. Filesystem VFS reads enforce mount-root containment.

## Build a game directory

```cmd
Scripts\Build\BuildGamePackage.cmd WeaponLab Shared\Intermediate\packages\WeaponLab
Scripts\Build\BuildGamePackage.cmd MadMaxWasteland Shared\Intermediate\packages\Saltwake low_memory
```

The wrapper builds the release engine/tool and invokes `package-build`. Existing output directories are refused. The tool first resolves/validates the project configuration and entry scene, inventories inputs, builds a sibling staging directory, verifies it, then publishes the final directory by rename. Input links and destinations inside an input root are refused.

```cmd
target\release\newviso-project-tool.exe package-plan --base-dir . --project ..\Projects\WeaponLab
target\release\newviso-project-tool.exe package-build --base-dir . --project ..\Projects\WeaponLab --output ..\Shared\Intermediate\packages\WeaponLab
target\release\newviso-project-tool.exe package-verify --package-dir ..\Shared\Intermediate\packages\WeaponLab
```

`--engine` explicitly selects an executable; otherwise the sibling `newviso.exe` is used. The output contains:

| Path | Contents |
|---|---|
| `newviso.exe` | Selected engine build |
| `providers/` | Deployed provider DLLs, formats, importers, toolchains and codecs |
| `shared/` | Full runtime-ready Shared Content |
| `game/` | Project manifest and authored project content |
| `config.json`, `Launch.cmd` | Relative deployment paths and launcher |
| `package.manifest.json` | Sorted relative file paths, lengths, BLAKE3 hashes, baked runtime location |

A separately configured codec root is copied to `codecs/`. Effective runtime settings, including the selected profile/CLI layers, are baked into the copied runtime file; profile selection in the copied manifest is cleared. Original project files are untouched. No absolute development paths or timestamps are added to the package manifest.

The first version uses a complete mounted-content closure. It intentionally includes unused Shared content/providers; it does not claim dependency pruning, archive compression, recooking, signing, or a target-platform cross-build. The current workspace's Shared Content is about 1.55 GB. All models/textures are copied byte-for-byte; this workflow generates no bitmap assets.

`package-verify` checks required files, every declared size/hash, duplicate/escaping paths, input links and untracked files. Runtime cache directories `game/.newviso/` and `cache/` are excluded from integrity checking. Launch with `Launch.cmd` on a compatible Windows machine with the required graphics/runtime prerequisites. A successful integrity check is not itself a native launch test.

## Architecture cleanup

Architecture rules now explicitly cover the contract/API crates and legitimate facade/domain dependencies. No general format-marker exception or console-output exemption was added. Concrete sky/bootstrap references moved into owning crate data. Skeleton sidecar locations are authored in `Shared/Content/config/engine/model_metadata.json`; engine code consumes the generic XML/decoded-body contract. Semantic texture references preserve the identities already emitted by the asset codec instead of branching on a source extension. Standalone benchmark/example exports use explicit `Write`; runtime logging remains provider-routed.

Native runtime settings and context actions are documented in [editor-project-settings.md](editor-project-settings.md).
