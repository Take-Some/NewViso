# Shared runtime snapshot transport

Lifecycle modules may opt in to runtime_delta in either their project entrypoint or a newviso.scripts.v1 module record. The default is false. Existing modules and providers continue to receive ordinary full JSON.

An opted-in root imports createRuntimeSnapshotDecoder from Shared's scripts/newviso/snapshot_delta.ysc, creates one decoder instance and calls it before its start/frame consumers. Imports can be shared by the provider, so the decoder instance belongs to the root, not to module-global shared state.

The engine sends a full base on first invocation, after successful module reload, or when that module's accepted revision differs from the transport base. Subsequent frames contain changed leaf values, property deletions or replacement arrays when the length changes. A field set to null differs from a removed field. Event hooks do not advance the lifecycle snapshot base.

The decoder reconstructs the same complete payload.runtime shape. It hides the transport envelope, preserves earlier retained snapshots and freezes runtime objects recursively. Opted-in consumers treat runtime data as read-only and request changes through commands. Frame input remains independently mutable. A decoder also accepts ordinary full JSON from older engines.

The transport is generic JSON and uses the existing provider-neutral ScriptClient request. It requires no provider ABI changes, scripting-language knowledge in the engine, or backend replacement. The deployed TypeScript provider resolves the Shared decoder through its existing relative YSC import support.

Snapshot comparison skips equal subtrees and borrows unchanged object keys, allocating path values only for actual changes. Floating-point signed zero and integer/float representation remain observable. ScriptRuntime takes ownership of frame state and retains it as the next base rather than cloning the entire descriptor tree. A rejected invocation does not commit the global base; a module that advanced before another module failed gets a full resynchronization on the next attempt.

Full engine runtime diagnostics retain every fragment and vehicle descriptor. Already-built nested JSON trees move directly into the final snapshot instead of being serialized into new trees again. Fragment mesh bounds are cached on installed meshes and invalidated before fragment vertex writes, including partial failure paths. Replacing or removing a model also replaces/removes that cache.

## Verification

    cargo test --offline -p newviso-scripting -p newviso-project --lib
    cargo test --offline -p newviso-runtime -p newviso-scene --lib

Rust regression checks cover nulls, deletion, type and array changes, authored property names, unchanged large descriptors and uncommitted rejected requests. scripts/tests/snapshot_delta.test.ysc supplies a deployed-provider integration check for independent roots, retained snapshots, read-only values, prototype-safe keys, revision rejection, empty deltas, array replacement, reset and full-JSON fallback. The original-settlement controlled probes invoke this check before gameplay start.
