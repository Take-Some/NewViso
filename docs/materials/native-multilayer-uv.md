# Native multilayer texture coordinates

This contract is implemented by NewViso's semantic model importer, common scene vertex format, GPU/CPU skinning, forward and deferred shading, and transparent-surface submission. It does not depend on Cyberpunk model names or project-specific patches.

## Vertex input

Every scene asset vertex carries both `TEXCOORD_0` (UV0) and `TEXCOORD_1` (UV1). The packed scene vertex has 19 floats (76 bytes): UV0 is at floats 11–12 and UV1 at 17–18. The shader vertex input for UV1 is location 9. Both `scene.vert` and `scene_instanced.vert` transmit the two interpolated UVs to location 5 (UV0) and 7 (UV1). CPU and GPU skinners preserve UV1 without interpolation or recomputing it. If a source mesh omits UV1, the decoder uses UV0 for both channels. It does **not** infer a fallback from the numerical contents of an explicitly supplied UV1 stream, which might legitimately be constant.

## Material contract

Any native model material with `"shader": "newviso.multilayer"` may specify independent texture coordinates:

```json
{
  "shader": "newviso.multilayer",
  "newviso_multilayer": {
    "layer_index": 3,
    "mat_tile": 1.0,
    "microblend_tile": 1.0,
    "uv_channels": {
      "color": 1,
      "normal": 1,
      "roughness": 1,
      "mask": 0,
      "microblend": 0
    }
  }
}
```

Each channel is an integer 0 or 1. The importer rejects invalid values. Unspecified roles default to the original mask/detail convention shown above. The resulting typed material parameters are `newviso_ml_color_uv`, `newviso_ml_normal_uv`, `newviso_ml_roughness_uv`, `newviso_ml_mask_uv`, and `newviso_ml_microblend_uv`. These are generic model properties; no vehicle-specific overrides are required.

At draw submission, selectors occupy bits 16–20 of the exactly representable material UBO integer flags, keeping the 12-float UBO ABI intact. Both forward and GBuffer shaders share `material_uv.glsl`, and ordinary PBR materials continue to sample UV0. Material details use authored tiling and offsets on their selected UV, while masks and microblend select their own UV before sampling. A missing source UV1 defaults to UV0; to use UV0 deliberately in a model with two coordinates, select 0 explicitly.

When a model lacks authored tangents, NewViso generates a tangent basis from the UV channel selected by its built-in multilayer normal map. Authored tangents are never overwritten.

## Layer ordering

Separate material layer draws remain distinct. The transparent main and water-reflection passes now share one comparator: when two layered draws reference exactly the same source surface and entity, their `layer_index` determines order (base to detail); otherwise ordinary back-to-front distance sorting applies. This also cooperates with shared immutable multilayer geometry without re-expanding every layer's vertices.

## Checks

- `cargo test -p newviso-scene --lib`
- `cargo test -p newviso-semantic-assets --lib`
- `cargo test -p newviso-runtime --lib`
- `python scripts/compile_scene_shaders.py`
- `cargo build --release -p newviso`

The interface supports two coordinates (UV0 and UV1) today. A third UV set or full REDengine multilayer blend semantics would require a further explicit extension; neither is silently emulated.

The source roughness level endpoints may extend outside 0–1 (e.g. for contrast curves). The importer supports finite [-8,8] endpoints and maps them back into the physical roughness range at shading time.
