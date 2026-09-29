# RSC7 source import -> NEF16 runtime asset pipeline

## Ownership

NewViso does not parse source file formats.

The runtime requests stable semantic assets from `engine.assets`. StarVault AssetManager owns source recognition, validates the source container and dispatches the bytes to a dynamically loaded codec worker.

~~~text
project / VFS
    |
    v
engine.assets (StarVault)
    |
    +--> engine.assets.types descriptor
    |       .ydr -> asset.codec.rsc7 -> model.runtime_v1
    |       .ybn -> asset.codec.rsc7 -> collision.runtime_v1
    |
    v
newengine-codec-rsc7
    |
    +--> RSC7/YDR -> NEF16 -> NVRM v1 + binary vertex/index payload
    +--> RSC7/YBN -> NEF16 -> NVRC v1 + binary collision payload
    +--> RSC7/YTD -> NEF16 -> NETD texture dictionary
    +--> RSC7/YPT -> NEF16 -> NVPT particle dictionary
    |
    v
asset.codec.listfile (NEF8 wire-v2 + NEF16 profile)
    |
    v
newviso-semantic-assets
    |
    +--> ModelResource      -> Scene / renderer
    +--> CollisionMeshResource -> physics
~~~

NEF16 is the canonical extended runtime profile for these imports. It keeps the NEF8 wire-v2 discovery prefix and uses the class-8 extension area, so existing NEF8 readers remain compatible when the profile declares legacy-safe semantics. RSC7 is source-only and never crosses into the runtime semantic layer.

The runtime bridge is deliberately source-format neutral. It decides whether it can materialize an asset only by the outputs advertised by the registered type descriptor. It does not branch on `.ydr`, `.ybn`, RSC7 magic, or a RSC7 type.

## YDR semantic contract

`model.runtime_v1` is encoded as an `NVRM` v1 frame:

~~~text
magic[4] = "NVRM"
version:u16 = 1
flags:u16
metadata_length:u32
payload_length:u64
metadata_json[metadata_length]
payload[payload_length]
~~~

The metadata schema is `northstar.model.runtime.v1`. It describes:

- model name and source identity;
- selected LOD and aggregate bounds;
- meshes and per-mesh bounds;
- position, normal, tangent, UV0, color0, joint-index and joint-weight streams;
- U32 index buffers;
- primitive ranges;
- source shader/material slot index.

Vertex and index bytes stay binary; large geometry is not serialized into JSON.

## YBN semantic contract

`collision.runtime_v1` is encoded as an `NVRC` v1 frame with the same framing layout. Its metadata schema is `northstar.collision.runtime.v1`.

It contains:

- collision name and bounds;
- packed `f32x3` vertices;
- packed `u32x3` triangle indices;
- one source physical-material id per triangle.

The semantic bridge validates all payload ranges before constructing `CollisionMeshResource`.

## Runtime contact surface recovery

The collision semantic resource preserves one opaque numeric physical-material id for every
authored triangle. NewViso keeps that table beside each resident streamed collision mesh.

The currently deployed replaceable Jolt provider predates surface/subshape fields in its contact
DTO, so the runtime does not require a provider ABI break. Jolt still supplies the authoritative
contact entities, point, normal and impulse. After each provider step, NewViso resolves the contact
point against the resident collision BVH, recovers the original authored triangle index, and
enriches the neutral contact event with optional `surface_id`, `surface_entity`, and `surface_triangle` fields.

This layer remains source-format neutral: core never interprets the numeric id as a RSC7 material
name. GTA/RSC7 compatibility data maps those ids to gameplay/audio classes in Shared content.
A future physics-provider ABI may return native subshape/surface identity directly; that can replace
the geometric recovery path without changing script event semantics.


## Address grammar

An `@` character is an asset selector only after the filename extension.

~~~text
models/world.asset@entry    selector = entry
maps/hi@district.asset      literal filename; no selector
~~~

This is required for real RSC7 filenames such as `hi@bh1_06_0.ybn`. The same rule is used by ResourceRuntime, the host type registry and AssetManager VFS/decode routing.

## Deployment

StarVault resolves `codecs/` and `formats/` relative to the deployed AssetManager provider DLL. For NewViso the required layout is:

~~~text
../pluginsRuntime/
    starVault-assetManager-*.dll
    codecs/
        newengine-codec-rsc7-0.1.0-release.dll
        codec_manifest.json
    formats/
        ydr.dll
        ybn.dll
~~~

Build, fixture-test and deploy the complete path with:

~~~powershell
python scripts/deploy_rsc7_asset_support.py
~~~

Use `--skip-tests` only when packaging a tree that was already verified.

## Current YBN coverage

The codec consumes `rsc7-formats 0.2.2` triangle extraction, including nested bound transforms and triangle physical-material ids. That library currently exposes triangle collision as the stable flattened representation. Non-triangle primitive polygons such as native spheres, capsules, boxes and cylinders require a later parser extension before they can be preserved as native primitive colliders instead of triangle-only coverage.

The semantic contract is intentionally independent of that source limitation, so additional collision primitive records can be introduced in a versioned extension without moving RSC7 parsing into NewViso.
