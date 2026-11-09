# Project configuration and profiles

NewViso has one typed runtime configuration model and one resolver shared by the live engine and the offline project tool. Profiles do not contain gameplay code and do not require a new executable.

## Ownership

- newviso-config owns generic JSON merging, source tracing, XML configuration decoding, and launch argument parsing.
- newviso-project owns the project configuration contract, profile inheritance, typed runtime settings, and validation.
- newviso-runtime supplies the live AssetClient/VFS reader and resolves settings ONCE before capability selection.
- newviso-project-tool supplies an offline filesystem reader; it does not link the renderer, platform, runtime, or provider loader.
- Shared/Content/config/engine/runtime.defaults.xml remains the authoritative complete base.
- Shared/Content/config/profiles contains reusable data presets. Projects opt into profile names and may replace the presets through normal VFS overlays.

No engine default values, gameplay presets, or concrete project names were added to Rust.

## Precedence

Later layers win:

1. Authoritative Shared runtime XML.
2. configuration.defaults, in order.
3. files.runtime.
4. configuration.platforms[current OS], in order.
5. Selected profile parents, then the selected profile's layers.
6. Explicit configuration.user_settings, if declared.
7. Repeated --runtime-layer arguments, in order.
8. Repeated --runtime-set assignments, in order.

Platform keys use Rust target OS names, such as windows, linux, macos.
The selected profile is --runtime-profile, then NEWVISO_RUNTIME_PROFILE, then configuration.default_profile.
No profile is selected implicitly when none of those is configured.

Objects merge recursively. Arrays and scalars replace as a whole. Null is a literal value, not deletion. To replace an array, supply the complete array. All authored layer documents require schema newviso.project.runtime.v1. The effective result passes the existing native typed validation.

Parents are traversed in declared order, with a common ancestor applied once in diamond inheritance. Cycles, unknown parents, unknown selected profiles, missing declared layers, malformed values, and unknown setting fields fail explicitly. Inheritance is bounded to 32 levels, profile catalogs to 128 entries, and the resolved file/override sequence to 256 entries in addition to the engine base.

The manifest's configuration object and each profile reject unknown fields. Existing manifests without configuration keep the previous Shared-plus-project behavior.

## Manifest example

~~~json
{
  "configuration": {
    "defaults": ["config/common.runtime.json"],
    "platforms": {
      "windows": ["config/windows.runtime.json"]
    },
    "profiles": {
      "balanced": {
        "layers": ["config/profiles/runtime.balanced.json"]
      },
      "low_memory": {
        "inherits": ["balanced"],
        "layers": ["config/profiles/runtime.low_memory.json"]
      },
      "high_throughput": {
        "inherits": ["balanced"],
        "layers": ["config/profiles/runtime.high_throughput.json"]
      }
    },
    "default_profile": "balanced",
    "user_settings": "config/user.runtime.json"
  }
}
~~~

Only declare files that exist. The user layer is explicit and is never written automatically. An undeclared user file is not loaded. A declared but missing file is an error.

A layer is a partial runtime document:

~~~json
{
  "schema": "newviso.project.runtime.v1",
  "streaming": {
    "parallel_loads": 4,
    "max_source_mb_per_tick": 16
  }
}
~~~

The Shared presets tune resource residency and streaming throughput only. They are starting points, not automatic hardware detection, VRAM guarantees, or verified performance improvements. max_resident_mb governs the existing engine residency policy and does not account for all backend/GPU allocations.

MadMaxWasteland registers balanced, low_memory and high_throughput as examples, without selecting a new default. Its existing runtime, scene, first-person gameplay and texture assets are retained.

## Launching

From the NorthStar root:

~~~bat
Scripts\Run\RunEngine.cmd MadMaxWasteland release low_memory
Scripts\Run\ProjectTool.cmd profiles MadMaxWasteland
Scripts\Run\ProjectTool.cmd validate MadMaxWasteland low_memory
Scripts\Run\ProjectTool.cmd resolve MadMaxWasteland low_memory
Scripts\Run\ProjectTool.cmd schema
~~~

The second RunEngine argument is the Cargo build profile. The third is the independent runtime settings profile.

Advanced native launch example, from NewViso:

~~~bat
target\debug\newviso.exe --base-dir . --project ..\Projects\MadMaxWasteland --runtime-profile low_memory --runtime-set streaming.parallel_loads=6
~~~

--runtime-layer names a logical VFS asset, not an arbitrary absolute file.
--runtime-set accepts a dotted object path or RFC 6901 JSON Pointer. The value is JSON when valid JSON; otherwise it is a plain string. Quote JSON strings explicitly when text such as true must remain text. Bootstrap --set remains a separate, allowlisted contract for paths/providers/boot switches.

~~~bat
target\debug\newviso.exe --project ..\Projects\MadMaxWasteland --runtime-set /variables/application/label=Review
~~~

Runtime override VALUES are not added to bootstrap debug messages. Explicit resolve output contains effective values; treat it like the underlying configuration files.

## Inspection and schema

newviso-project-tool supports:

- profiles: manifest profile definitions and declared configuration.
- resolve: effective settings, ordered layers, per-field origins, physical file bindings.
- validate: manifest and effective runtime configuration validation. It does not certify scenes, textures, providers or gameplay.
- schema: JSON Schema metadata for complete runtime documents, including live/restart section timing.

The offline reader honors priorities and logical mount prefixes from Shared's vfs_mounts.xml. It reads the three canonical filesystem mounts and refuses to fall through a malformed higher-priority file. Custom archive/provider-only mounts are outside the offline reader's scope.

Runtime snapshots expose project.launch_configuration. field_sources keys are RFC 6901 JSON Pointers; arrays have one origin. This report describes launch composition. Later startup commands and runtime.configure patches change CURRENT settings and remain observable through runtime.settings and runtime.settings.changed. The launch report deliberately does not claim to describe those later writes.

Shared's RuntimeConfiguration<T> exposes launchConfiguration and launchSourceOf(path):

~~~typescript
import { RuntimeConfiguration } from "./newviso/runtime.ysc";

const configuration = new RuntimeConfiguration(payload);
const profile = configuration.launchConfiguration?.profile;
const origin = configuration.launchSourceOf("streaming.parallel_loads");
return {
  commands: [
    configuration.configure({ streaming: { parallel_loads: 6 } })
  ]
};
~~~

Supported live settings retain atomic native validation. Window size, persistence configuration and startup-only command arrays require restart. A profile's launch selection is not an automatic live profile switch.

## Extending the model

For a new native setting, update the owning typed settings structure, validation, its real consumer and runtime-settings.schema.json. The schema coverage test catches fields missing from the metadata. Camera limit relationships and byte-budget overflow remain additional native validation constraints.

Gameplay values stay under variables and are validated by their consumer, rather than being promoted into engine policy. Provider selection retains its existing bootstrap/project contract; changing the renderer or physics implementation still requires a compatible provider.

## Remaining engine work

This foundation supplies configuration contracts, profiles, inspection and editor metadata. It does not implement a graphical Editor, reflection for arbitrary scene components, asset cooking/build targets, package manifests, a visual material editor, animation graph editing, undo/redo, or live provider replacement.

Those systems should consume these contracts and the existing AssetManager, provider and scene APIs. They should not create another settings parser or a separate project runtime.

## Verified implementation (2026-10-16)

26 unit tests in the affected configuration/project packages passed. Workspace cargo check, debug and release application builds passed. All eight active project manifests/runtime configurations validated without selecting a new default profile. The native Vulkan/TypeScript verification asserted inherited profile selection, CLI precedence, an 800-pixel launch window, launch-source reporting and a live runtime.configure patch; the engine exited normally after nine platform frames. The Windows ProjectTool.cmd wrapper validated low_memory successfully. The 23 gameplay modules retained their source hashes.

The existing global architecture-boundary and direct-console-output checks still report pre-existing findings in other engine areas. This change does not certify those unrelated gates as clean. See .newviso/cache/configuration-proof for generated verification results.

The weapon regression on the rebuilt release engine also passed: three weapons, reload, nine destroyed targets, 144 physical fragments, and attached/moving/removed hit marks. Final generated acceptance report: .newviso/cache/engine-configuration.acceptance.json.
