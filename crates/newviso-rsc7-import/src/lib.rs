mod character_animation;
mod character_skeleton;
mod material;

pub use character_animation::load_model_animation_clip;
pub use material::SemanticMaterialDecoder;

use newviso_assets_client::AssetClient;
use newviso_collision::{CollisionBounds, CollisionMeshResource};
use newviso_materials::{
    BlendMode, MaterialParamValue, MaterialParameter, MaterialResource, MaterialTextureBinding,
};
use newviso_model::{
    Bounds3, IndexBuffer, IndexFormat, MeshPrimitive, MeshResource, ModelFragmentLight,
    ModelFragmentMetadata, ModelFragmentPart, ModelFragmentPartRole, ModelJoint,
    ModelMaterialBinding, ModelMaterialSlot, ModelResource, ModelSkeleton, ModelWheelSlot,
    VertexFormat, VertexSemantic, VertexStream,
};
use newviso_resource_runtime::{AssetAddress, AssetId, AssetRef, AssetResource, ResourceDecoder};
use newviso_textures::{TextureColorSpace, TextureFormat, TextureMip, TextureResource};
use serde_json::{json, Value};
use std::sync::Arc;

const MODEL_RUNTIME_OUTPUT: &str = "model.runtime_v1";
const COLLISION_RUNTIME_OUTPUT: &str = "collision.runtime_v1";
const TEXTURE_RUNTIME_OUTPUT: &str = "assets.textures.entry_runtime_v1";
const TEXTURE_RUNTIME_OUTPUT_ALIAS: &str = "texture.runtime";
const TEXTURE_WIRE_MAGIC: [u8; 4] = *b"NTRT";
const TEXTURE_WIRE_VERSION: u16 = 2;
const TEXTURE_WIRE_HEADER_LEN: usize = 32;
const TEXTURE_MIP_RECORD_LEN: usize = 20;
const MODEL_WIRE_MAGIC: [u8; 4] = *b"NVRM";
const COLLISION_WIRE_MAGIC: [u8; 4] = *b"NVRC";
const WIRE_VERSION: u16 = 1;
const WIRE_HEADER_LEN: usize = 20;

#[derive(Clone, Debug)]
struct ImportedTextureBinding {
    role: String,
    texture_name: String,
    texture: Option<AssetRef<TextureResource>>,
    required: bool,
}

#[derive(Clone, Debug)]
struct ImportedMaterialParameter {
    source_parameter_hash: u32,
    values: Vec<[f32; 4]>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ImportedMaterialShading {
    normal_strength: f32,
    specular_intensity: f32,
    specular_falloff: f32,
    specular_fresnel: f32,
    emissive_multiplier: f32,
    environment_reflection: f32,
    alpha_cutoff: f32,
    opacity: f32,
}

/// Source-format-neutral semantic adapter. AssetManager owns source parsing;
/// this runtime layer materializes the stable generic model contract only.
#[derive(Clone, Copy, Debug, Default)]
pub struct SemanticModelDecoder;

impl ResourceDecoder for SemanticModelDecoder {
    fn name(&self) -> &'static str {
        "newviso.asset_manager.model_semantic.v1"
    }

    fn decode(
        &self,
        address: &AssetAddress,
        _bytes: &[u8],
    ) -> Result<Option<Arc<dyn AssetResource>>, String> {
        let assets = AssetClient::new();
        let type_info = assets.resolve_type(address.logical_path())?;
        if !descriptor_supports(&type_info, MODEL_RUNTIME_OUTPUT) {
            return Ok(None);
        }

        let selector = address
            .entry()
            .map(|entry| json!({"entry": entry}))
            .unwrap_or(Value::Null);
        let wire = assets.decode(address.logical_path(), MODEL_RUNTIME_OUTPUT, selector)?;
        let (meta, payload) = unpack_semantic_wire(&wire, MODEL_WIRE_MAGIC)?;
        let mut model = decode_model_resource(address, &meta, payload)?;
        character_skeleton::attach_character_skeleton(&assets, address, &mut model)?;
        Ok(Some(Arc::new(model)))
    }
}

/// Source-format-neutral semantic adapter for runtime-ready texture entries.
/// AssetManager owns YTD/source decoding and returns the normalized NTRT v2 packet.
#[derive(Clone, Copy, Debug, Default)]
pub struct SemanticTextureDecoder;

impl ResourceDecoder for SemanticTextureDecoder {
    fn name(&self) -> &'static str {
        "newviso.asset_manager.texture_semantic.v1"
    }

    fn decode(
        &self,
        address: &AssetAddress,
        _bytes: &[u8],
    ) -> Result<Option<Arc<dyn AssetResource>>, String> {
        let Some(entry) = address.entry() else {
            return Ok(None);
        };
        let assets = AssetClient::new();
        let type_info = assets.resolve_type(address.logical_path())?;
        let is_texture_dictionary = type_info
            .get("descriptor")
            .and_then(|descriptor| descriptor.get("asset_kind"))
            .and_then(Value::as_str)
            .is_some_and(|kind| kind == "texture_dictionary");
        if !is_texture_dictionary {
            return Ok(None);
        }
        let output = if descriptor_supports(&type_info, TEXTURE_RUNTIME_OUTPUT) {
            TEXTURE_RUNTIME_OUTPUT
        } else {
            TEXTURE_RUNTIME_OUTPUT_ALIAS
        };
        let wire = assets.decode(
            address.logical_path(),
            output,
            json!({"texture_name": entry}),
        )?;
        let texture = decode_texture_resource(address, entry, &wire)?;
        Ok(Some(Arc::new(texture)))
    }
}

fn normalize_rsc7_texture_address(address: AssetAddress) -> AssetAddress {
    // GTA texture dictionary entry identity is case-insensitive, while the
    // canonical NETD dictionaries are authored with lowercase entry names.
    // Normalize only YTD entry selectors here; never alter selectors for other
    // asset families where case may be semantically meaningful.
    let is_ytd = address
        .logical_path()
        .rsplit_once('.')
        .is_some_and(|(_, extension)| extension.eq_ignore_ascii_case("ytd"));
    let Some(entry) = address.entry().filter(|_| is_ytd) else {
        return address;
    };
    let normalized_entry = entry.to_ascii_lowercase();
    if normalized_entry == entry {
        return address;
    }
    AssetAddress::parse(&format!("{}@{}", address.logical_path(), normalized_entry))
        .unwrap_or(address)
}

fn decode_texture_resource(
    address: &AssetAddress,
    entry: &str,
    bytes: &[u8],
) -> Result<TextureResource, String> {
    if bytes.len() < TEXTURE_WIRE_HEADER_LEN || bytes[..4] != TEXTURE_WIRE_MAGIC {
        return Err(format!(
            "texture '{}' returned invalid NTRT runtime packet",
            address.canonical()
        ));
    }
    let version = u16::from_le_bytes([bytes[4], bytes[5]]);
    if version != TEXTURE_WIRE_VERSION {
        return Err(format!(
            "texture '{}' returned NTRT version {}, expected {}",
            address.canonical(),
            version,
            TEXTURE_WIRE_VERSION
        ));
    }
    let format_id = u16::from_le_bytes([bytes[8], bytes[9]]);
    let format = match format_id {
        1 => TextureFormat::Rgba8Unorm,
        2 => TextureFormat::Rgba8Srgb,
        101 => TextureFormat::Bc1RgbaUnorm,
        102 => TextureFormat::Bc1RgbaSrgb,
        103 => TextureFormat::Bc3RgbaUnorm,
        104 => TextureFormat::Bc3RgbaSrgb,
        105 => TextureFormat::Bc5RgUnorm,
        106 => TextureFormat::Bc7RgbaUnorm,
        107 => TextureFormat::Bc7RgbaSrgb,
        other => {
            return Err(format!(
                "texture '{}' uses unsupported runtime format id {}",
                address.canonical(),
                other
            ))
        }
    };
    let mip_count = u16::from_le_bytes([bytes[10], bytes[11]]) as usize;
    let width = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
    let height = u32::from_le_bytes(bytes[16..20].try_into().unwrap());
    let payload_len = u32::from_le_bytes(bytes[20..24].try_into().unwrap()) as usize;
    let records_len = mip_count
        .checked_mul(TEXTURE_MIP_RECORD_LEN)
        .ok_or_else(|| "texture mip table overflow".to_owned())?;
    let payload_offset = TEXTURE_WIRE_HEADER_LEN
        .checked_add(records_len)
        .ok_or_else(|| "texture payload offset overflow".to_owned())?;
    if bytes.len() != payload_offset.saturating_add(payload_len) {
        return Err(format!(
            "texture '{}' NTRT size mismatch bytes={} expected={}",
            address.canonical(),
            bytes.len(),
            payload_offset.saturating_add(payload_len)
        ));
    }
    let payload = &bytes[payload_offset..];
    let mut mips = Vec::with_capacity(mip_count);
    for index in 0..mip_count {
        let base = TEXTURE_WIRE_HEADER_LEN + index * TEXTURE_MIP_RECORD_LEN;
        let level = u16::from_le_bytes(bytes[base..base + 2].try_into().unwrap()) as u32;
        let mip_width = u32::from_le_bytes(bytes[base + 4..base + 8].try_into().unwrap());
        let mip_height = u32::from_le_bytes(bytes[base + 8..base + 12].try_into().unwrap());
        let offset = u32::from_le_bytes(bytes[base + 12..base + 16].try_into().unwrap()) as usize;
        let len = u32::from_le_bytes(bytes[base + 16..base + 20].try_into().unwrap()) as usize;
        let end = offset
            .checked_add(len)
            .ok_or_else(|| "texture mip range overflow".to_owned())?;
        let data = payload.get(offset..end).ok_or_else(|| {
            format!(
                "texture '{}' mip {} range {}..{} exceeds payload={}",
                address.canonical(),
                level,
                offset,
                end,
                payload.len()
            )
        })?;
        let expected = format
            .expected_mip_bytes(mip_width, mip_height)
            .ok_or_else(|| "texture mip size overflow".to_owned())?;
        if expected != data.len() {
            return Err(format!(
                "texture '{}' mip {} bytes={} expected={} extent={}x{}",
                address.canonical(),
                level,
                data.len(),
                expected,
                mip_width,
                mip_height
            ));
        }
        mips.push(TextureMip {
            level,
            width: mip_width,
            height: mip_height,
            data: Arc::from(data),
        });
    }
    if mips.is_empty() {
        return Err(format!(
            "texture '{}' contains no mip levels",
            address.canonical()
        ));
    }
    Ok(TextureResource {
        id: AssetId::from_address(address),
        name: entry.to_owned(),
        name_hash: 0,
        width,
        height,
        format,
        color_space: if format.is_srgb() {
            TextureColorSpace::Srgb
        } else {
            TextureColorSpace::Linear
        },
        mips,
    })
}

/// Source-format-neutral semantic adapter. AssetManager owns source parsing;
/// this layer materializes the generic collision contract for physics consumers.
#[derive(Clone, Copy, Debug, Default)]
pub struct SemanticCollisionDecoder;

impl ResourceDecoder for SemanticCollisionDecoder {
    fn name(&self) -> &'static str {
        "newviso.asset_manager.collision_semantic.v1"
    }

    fn decode(
        &self,
        address: &AssetAddress,
        _bytes: &[u8],
    ) -> Result<Option<Arc<dyn AssetResource>>, String> {
        let assets = AssetClient::new();
        let type_info = assets.resolve_type(address.logical_path())?;
        if !descriptor_supports(&type_info, COLLISION_RUNTIME_OUTPUT) {
            return Ok(None);
        }

        let wire = assets.decode(
            address.logical_path(),
            COLLISION_RUNTIME_OUTPUT,
            Value::Null,
        )?;
        let (meta, payload) = unpack_semantic_wire(&wire, COLLISION_WIRE_MAGIC)?;
        let collision = decode_collision_resource(address, &meta, payload)?;
        Ok(Some(Arc::new(collision)))
    }
}

fn descriptor_supports(type_info: &Value, output: &str) -> bool {
    if type_info.get("known").and_then(Value::as_bool) != Some(true) {
        return false;
    }
    type_info
        .get("descriptor")
        .and_then(|value| value.get("outputs"))
        .and_then(Value::as_array)
        .is_some_and(|outputs| {
            outputs
                .iter()
                .any(|candidate| candidate.as_str() == Some(output))
        })
}

fn unpack_semantic_wire(bytes: &[u8], expected_magic: [u8; 4]) -> Result<(Value, &[u8]), String> {
    if bytes.len() < WIRE_HEADER_LEN {
        return Err(format!(
            "semantic asset wire is truncated: bytes={} expected_header={WIRE_HEADER_LEN}",
            bytes.len()
        ));
    }
    if bytes[..4] != expected_magic {
        return Err(format!(
            "semantic asset wire magic mismatch: actual={:?} expected={:?}",
            &bytes[..4],
            expected_magic
        ));
    }
    let version = u16::from_le_bytes([bytes[4], bytes[5]]);
    if version != WIRE_VERSION {
        return Err(format!(
            "unsupported semantic asset wire version {version}, expected {WIRE_VERSION}"
        ));
    }
    let meta_len = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
    let payload_len_u64 = u64::from_le_bytes(bytes[12..20].try_into().unwrap());
    let payload_len = usize::try_from(payload_len_u64)
        .map_err(|_| "semantic payload length exceeds usize".to_owned())?;
    let meta_end = WIRE_HEADER_LEN
        .checked_add(meta_len)
        .ok_or_else(|| "semantic metadata range overflow".to_owned())?;
    let payload_end = meta_end
        .checked_add(payload_len)
        .ok_or_else(|| "semantic payload range overflow".to_owned())?;
    if payload_end != bytes.len() {
        return Err(format!(
            "semantic asset wire length mismatch: header declares {} bytes, actual {}",
            payload_end,
            bytes.len()
        ));
    }
    let meta: Value = serde_json::from_slice(&bytes[WIRE_HEADER_LEN..meta_end])
        .map_err(|error| format!("semantic metadata JSON is invalid: {error}"))?;
    Ok((meta, &bytes[meta_end..payload_end]))
}

fn decode_model_resource(
    address: &AssetAddress,
    meta: &Value,
    payload: &[u8],
) -> Result<ModelResource, String> {
    require_schema(meta, "northstar.model.runtime.v1")?;
    let name = required_str(meta, "name")?.to_owned();
    let bounds = decode_bounds(required(meta, "bounds")?)?;
    let mesh_values = required(meta, "meshes")?
        .as_array()
        .ok_or_else(|| "model meshes must be an array".to_owned())?;
    if mesh_values.is_empty() {
        return Err("model semantic payload contains no meshes".to_owned());
    }

    let mut meshes = Vec::with_capacity(mesh_values.len());
    // Vehicle fragment splitting intentionally reuses authored vertex streams
    // across many rigid semantic index groups. Share identical wire slices in
    // memory instead of allocating one Arc payload per split mesh.
    let mut shared_stream_data = std::collections::BTreeMap::<(usize, usize), Arc<[u8]>>::new();
    for (mesh_index, mesh) in mesh_values.iter().enumerate() {
        let mesh_name = required_str(mesh, "name")
            .map_err(|error| format!("mesh[{mesh_index}] {error}"))?
            .to_owned();
        let vertex_count = required_u32(mesh, "vertex_count")
            .map_err(|error| format!("mesh[{mesh_index}] {error}"))?;
        let mesh_bounds = decode_bounds(required(mesh, "bounds")?)
            .map_err(|error| format!("mesh[{mesh_index}] {error}"))?;

        let stream_values = required(mesh, "streams")?
            .as_array()
            .ok_or_else(|| format!("mesh[{mesh_index}] streams must be an array"))?;
        let mut vertex_streams = Vec::with_capacity(stream_values.len());
        for (stream_index, stream) in stream_values.iter().enumerate() {
            let semantic = parse_semantic(required_str(stream, "semantic")?)?;
            let format = parse_vertex_format(required_str(stream, "format")?)?;
            let stride = required_u32(stream, "stride")?;
            let data = payload_slice(payload, stream, "stream")?;
            let stream_offset = required_usize(stream, "offset")?;
            let stream_length = required_usize(stream, "length")?;
            let expected_min = usize::try_from(vertex_count)
                .ok()
                .and_then(|count| count.checked_mul(stride as usize))
                .ok_or_else(|| "vertex stream size overflow".to_owned())?;
            if data.len() < expected_min {
                return Err(format!(
                    "mesh[{mesh_index}] stream[{stream_index}] has {} bytes, expected at least {}",
                    data.len(),
                    expected_min
                ));
            }
            let shared = shared_stream_data
                .entry((stream_offset, stream_length))
                .or_insert_with(|| Arc::from(data))
                .clone();
            vertex_streams.push(VertexStream {
                semantic,
                format,
                stride,
                vertex_count,
                data: shared,
            });
        }

        let index_meta = required(mesh, "index_buffer")?;
        let index_format = match required_str(index_meta, "format")? {
            "u16" => IndexFormat::U16,
            "u32" => IndexFormat::U32,
            other => return Err(format!("unsupported index format '{other}'")),
        };
        let index_count = required_u32(index_meta, "index_count")?;
        let index_data = payload_slice(payload, index_meta, "index buffer")?;
        let index_stride = match index_format {
            IndexFormat::U16 => 2usize,
            IndexFormat::U32 => 4usize,
        };
        let expected_index_bytes = (index_count as usize)
            .checked_mul(index_stride)
            .ok_or_else(|| "index buffer size overflow".to_owned())?;
        if index_data.len() < expected_index_bytes {
            return Err(format!(
                "mesh[{mesh_index}] index buffer has {} bytes, expected at least {}",
                index_data.len(),
                expected_index_bytes
            ));
        }

        let primitive_values = required(mesh, "primitives")?
            .as_array()
            .ok_or_else(|| format!("mesh[{mesh_index}] primitives must be an array"))?;
        let primitives = primitive_values
            .iter()
            .map(|primitive| {
                Ok(MeshPrimitive {
                    first_index: required_u32(primitive, "first_index")?,
                    index_count: required_u32(primitive, "index_count")?,
                    base_vertex: required_i32(primitive, "base_vertex")?,
                    material_slot: primitive
                        .get("material_slot")
                        .and_then(Value::as_u64)
                        .and_then(|value| u32::try_from(value).ok()),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;

        meshes.push(MeshResource {
            name: mesh_name,
            vertex_streams,
            index_buffer: IndexBuffer {
                format: index_format,
                index_count,
                data: Arc::from(index_data),
            },
            primitives,
            bounds: mesh_bounds,
        });
    }

    let material_slots = meta
        .get("material_slots")
        .and_then(Value::as_array)
        .map(|slots| {
            slots
                .iter()
                .enumerate()
                .map(|(slot_index, slot)| {
                    let name = slot
                        .get("name")
                        .or_else(|| slot.get("slot_name"))
                        .and_then(Value::as_str)
                        .unwrap_or("material")
                        .to_owned();
                    let authored_material_ref = slot
                        .get("material_ref")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(str::to_owned)
                        .or_else(|| {
                            // Native YDD v2 stores the material selector in the
                            // material-slot string. Treat canonical material
                            // addresses as external resources instead of a
                            // literal slot label so modern .ymat@entry
                            // bindings survive the YDD -> semantic projection.
                            let candidate = name.trim();
                            let lower = candidate.to_ascii_lowercase();
                            ((lower.contains(".ymat@") || lower.contains(".nemat@"))
                                && candidate.contains('@'))
                            .then(|| candidate.to_owned())
                        });
                    let external_material = authored_material_ref
                        .as_deref()
                        .map(AssetAddress::parse)
                        .transpose()
                        .map_err(|error| {
                            format!(
                                "material_slot[{slot_index}] material reference invalid: {error}"
                            )
                        })?
                        .map(AssetRef::new);
                    let textures = slot
                        .get("textures")
                        .and_then(Value::as_array)
                        .map(|textures| {
                            textures
                                .iter()
                                .filter_map(|texture| {
                                    let texture_name = texture.get("name")?.as_str()?.trim();
                                    if texture_name.is_empty() {
                                        return None;
                                    }
                                    let texture_ref = texture
                                        .get("asset_ref")
                                        .and_then(Value::as_str)
                                        .map(AssetAddress::parse)
                                        .transpose()
                                        .ok()
                                        .flatten()
                                        .map(normalize_rsc7_texture_address)
                                        .map(AssetRef::new);
                                    let role = texture
                                        .get("role")
                                        .and_then(Value::as_str)
                                        .unwrap_or("generic")
                                        .to_owned();
                                    let required = texture
                                        .get("required")
                                        .and_then(Value::as_bool)
                                        .unwrap_or_else(|| {
                                            !matches!(
                                                role.trim().to_ascii_lowercase().as_str(),
                                                "environment" | "environment_map" | "reflection"
                                            )
                                        });
                                    Some(ImportedTextureBinding {
                                        role,
                                        texture_name: texture_name.to_owned(),
                                        texture: texture_ref,
                                        required,
                                    })
                                })
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    let parameters = slot
                        .get("parameters")
                        .and_then(Value::as_array)
                        .map(|parameters| {
                            parameters
                                .iter()
                                .filter_map(|parameter| {
                                    let source_parameter_hash = parameter
                                        .get("source_parameter_hash")
                                        .and_then(Value::as_u64)
                                        .and_then(|value| u32::try_from(value).ok())?;
                                    let values = parameter
                                        .get("values")
                                        .and_then(Value::as_array)?
                                        .iter()
                                        .filter_map(|value| {
                                            let components = value.as_array()?;
                                            if components.len() != 4 {
                                                return None;
                                            }
                                            let mut out = [0.0f32; 4];
                                            for (index, component) in components.iter().enumerate()
                                            {
                                                let component = component.as_f64()? as f32;
                                                if !component.is_finite() {
                                                    return None;
                                                }
                                                out[index] = component;
                                            }
                                            Some(out)
                                        })
                                        .collect::<Vec<_>>();
                                    Some(ImportedMaterialParameter {
                                        source_parameter_hash,
                                        values,
                                    })
                                })
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    let shading = decode_material_shading(slot, &parameters);
                    let surface_domain = slot
                        .get("surface_domain")
                        .and_then(Value::as_str)
                        .unwrap_or("surface")
                        .to_owned();
                    let blend_mode = slot
                        .get("blend_mode")
                        .and_then(Value::as_str)
                        .unwrap_or("opaque")
                        .to_owned();
                    let material = match external_material {
                        Some(reference) => ModelMaterialBinding::External(reference),
                        None => ModelMaterialBinding::BuiltIn(Arc::new(
                            build_builtin_material_resource(
                                address,
                                slot_index,
                                &name,
                                slot,
                                &surface_domain,
                                &blend_mode,
                                &textures,
                                shading,
                            )?,
                        )),
                    };
                    Ok(ModelMaterialSlot { name, material })
                })
                .collect::<Result<Vec<_>, String>>()
        })
        .transpose()?
        .unwrap_or_default();

    let fragment = decode_optional_fragment(meta.get("fragment"), &meshes)?;
    let skin_source_to_model = decode_optional_mat4(meta.get("skin_source_to_model"))?;
    let skeleton = decode_optional_skeleton(meta.get("skeleton"))?;
    Ok(ModelResource {
        id: AssetId::from_address(address),
        name,
        bounds,
        meshes,
        material_slots,
        skin_source_to_model,
        skeleton,
        animations: Vec::new(),
        fragment,
    })
}

fn decode_optional_skeleton(value: Option<&Value>) -> Result<Option<ModelSkeleton>, String> {
    let Some(value) = value.filter(|value| !value.is_null()) else {
        return Ok(None);
    };
    let schema = required_str(value, "schema")?;
    if schema != "northstar.model.skeleton.v1" {
        return Err(format!(
            "unsupported model skeleton schema '{schema}', expected 'northstar.model.skeleton.v1'"
        ));
    }
    let name = required_str(value, "name")?.to_owned();
    let joint_values = required(value, "joints")?
        .as_array()
        .ok_or_else(|| "model skeleton joints must be an array".to_owned())?;
    if joint_values.is_empty() {
        return Err("model skeleton contains no joints".to_owned());
    }
    if joint_values.len() > u16::MAX as usize {
        return Err("model skeleton joint count exceeds u16".to_owned());
    }

    let mut joints = Vec::with_capacity(joint_values.len());
    for (index, joint) in joint_values.iter().enumerate() {
        let parent = joint
            .get("parent")
            .filter(|value| !value.is_null())
            .map(|value| {
                value
                    .as_u64()
                    .ok_or_else(|| format!("skeleton joint[{index}] parent must be unsigned"))
                    .and_then(|value| {
                        u16::try_from(value)
                            .map_err(|_| format!("skeleton joint[{index}] parent exceeds u16"))
                    })
            })
            .transpose()?;
        if parent.is_some_and(|parent| parent as usize >= index) {
            return Err(format!(
                "skeleton joint[{index}] parent must reference an earlier dense joint"
            ));
        }
        let bind_rotation = required_vec4(joint, "bind_rotation")?;
        let rotation_norm2 = bind_rotation.iter().map(|value| value * value).sum::<f32>();
        if !rotation_norm2.is_finite() || rotation_norm2 <= 1.0e-8 {
            return Err(format!("skeleton joint[{index}] bind rotation is invalid"));
        }
        let bind_scale = required_vec3(joint, "bind_scale")?;
        if bind_scale.iter().any(|value| value.abs() <= 1.0e-8) {
            return Err(format!("skeleton joint[{index}] bind scale is singular"));
        }
        joints.push(ModelJoint {
            name: required_str(joint, "name")?.to_owned(),
            tag: required_u32(joint, "tag")?,
            parent,
            inverse_bind_matrix: decode_required_mat4(joint, "inverse_bind_matrix")?,
            bind_translation: required_vec3(joint, "bind_translation")?,
            bind_rotation,
            bind_scale,
        });
    }
    Ok(Some(ModelSkeleton { name, joints }))
}

fn decode_optional_fragment(
    value: Option<&Value>,
    meshes: &[MeshResource],
) -> Result<Option<ModelFragmentMetadata>, String> {
    let Some(value) = value.filter(|value| !value.is_null()) else {
        return Ok(None);
    };
    let schema = required_str(value, "schema")?;
    if schema != "northstar.model.fragment.v1" {
        return Err(format!(
            "unsupported model fragment schema '{schema}', expected 'northstar.model.fragment.v1'"
        ));
    }
    let bound_center = required_vec3(value, "bound_center")?;
    let bound_radius = value
        .get("bound_radius")
        .and_then(Value::as_f64)
        .map(|value| value as f32)
        .filter(|value| value.is_finite() && *value >= 0.0)
        .ok_or_else(|| "fragment bound_radius must be finite and non-negative".to_owned())?;
    let parts = required(value, "parts")?
        .as_array()
        .ok_or_else(|| "fragment parts must be an array".to_owned())?
        .iter()
        .enumerate()
        .map(|(part_index, part)| {
            let index = part
                .get("index")
                .and_then(Value::as_u64)
                .and_then(|value| u32::try_from(value).ok())
                .unwrap_or(part_index as u32);
            let name = required_str(part, "name")?.to_owned();
            let role = match required_str(part, "role")? {
                "body" => ModelFragmentPartRole::Body,
                "wheel" => ModelFragmentPartRole::Wheel,
                "suspension" => ModelFragmentPartRole::Suspension,
                "wheel_hub" => ModelFragmentPartRole::WheelHub,
                "door" => ModelFragmentPartRole::Door,
                "bonnet" => ModelFragmentPartRole::Bonnet,
                "boot" => ModelFragmentPartRole::Boot,
                "glass" => ModelFragmentPartRole::Glass,
                "body_panel" => ModelFragmentPartRole::BodyPanel,
                "breakable" => ModelFragmentPartRole::Breakable,
                "extra" => ModelFragmentPartRole::Extra,
                "light" => ModelFragmentPartRole::Light,
                "siren" => ModelFragmentPartRole::Siren,
                "exhaust" => ModelFragmentPartRole::Exhaust,
                "engine" => ModelFragmentPartRole::Engine,
                "seat" => ModelFragmentPartRole::Seat,
                "weapon_mount" => ModelFragmentPartRole::WeaponMount,
                "roof" => ModelFragmentPartRole::Roof,
                "spoiler" => ModelFragmentPartRole::Spoiler,
                "steering" => ModelFragmentPartRole::Steering,
                "part" => ModelFragmentPartRole::Part,
                other => {
                    return Err(format!(
                        "fragment part[{part_index}] has unsupported role '{other}'"
                    ))
                }
            };
            let bone_tag = part
                .get("bone_tag")
                .and_then(Value::as_u64)
                .map(u32::try_from)
                .transpose()
                .map_err(|_| format!("fragment part[{part_index}] bone_tag exceeds u32"))?;
            let group_index = part
                .get("group_index")
                .and_then(Value::as_u64)
                .map(u16::try_from)
                .transpose()
                .map_err(|_| format!("fragment part[{part_index}] group_index exceeds u16"))?;
            let parent_part_index = part
                .get("parent_index")
                .and_then(Value::as_u64)
                .map(u32::try_from)
                .transpose()
                .map_err(|_| format!("fragment part[{part_index}] parent_index exceeds u32"))?;
            let wheel_slot = match part.get("wheel_slot").and_then(Value::as_str) {
                None => None,
                Some("front_left") => Some(ModelWheelSlot::FrontLeft),
                Some("front_right") => Some(ModelWheelSlot::FrontRight),
                Some("rear_left") => Some(ModelWheelSlot::RearLeft),
                Some("rear_right") => Some(ModelWheelSlot::RearRight),
                Some(other) => {
                    return Err(format!(
                        "fragment part[{part_index}] has unsupported wheel_slot '{other}'"
                    ))
                }
            };
            let mesh_names = required(part, "mesh_names")?
                .as_array()
                .ok_or_else(|| format!("fragment part[{part_index}] mesh_names must be an array"))?
                .iter()
                .map(|value| {
                    value.as_str().map(str::to_owned).ok_or_else(|| {
                        format!("fragment part[{part_index}] mesh_names entries must be strings")
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            for mesh_name in &mesh_names {
                if !meshes.iter().any(|mesh| mesh.name == *mesh_name) {
                    return Err(format!(
                        "fragment part[{part_index}] references missing mesh '{mesh_name}'"
                    ));
                }
            }
            let rest_transform = decode_required_mat4(part, "rest_transform")?;
            let rest_position = required_vec3(part, "rest_position")?;
            Ok(ModelFragmentPart {
                index,
                name,
                role,
                bone_tag,
                group_index,
                parent_part_index,
                wheel_slot,
                mesh_names,
                rest_transform,
                rest_position,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;

    let lights = value
        .get("lights")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
        .iter()
        .enumerate()
        .map(|(light_index, light)| {
            let index = light
                .get("index")
                .and_then(Value::as_u64)
                .and_then(|value| u32::try_from(value).ok())
                .unwrap_or(light_index as u32);
            let part_index = light
                .get("part_index")
                .and_then(Value::as_u64)
                .map(u32::try_from)
                .transpose()
                .map_err(|_| format!("fragment light[{light_index}] part_index exceeds u32"))?;
            let bone_index = required_u16(light, "bone_index")
                .map_err(|error| format!("fragment light[{light_index}] {error}"))?;
            let bone_name = light
                .get("bone_name")
                .and_then(Value::as_str)
                .map(str::to_owned);
            let light_type = required_u8(light, "light_type")
                .map_err(|error| format!("fragment light[{light_index}] {error}"))?;
            let group_id = required_u8(light, "group_id")
                .map_err(|error| format!("fragment light[{light_index}] {error}"))?;
            let position = required_vec3(light, "position")
                .map_err(|error| format!("fragment light[{light_index}] {error}"))?;
            let direction = required_vec3(light, "direction")
                .map_err(|error| format!("fragment light[{light_index}] {error}"))?;
            let tangent = required_vec3(light, "tangent")
                .map_err(|error| format!("fragment light[{light_index}] {error}"))?;
            let color = required_vec3(light, "color")
                .map_err(|error| format!("fragment light[{light_index}] {error}"))?;
            let intensity = required_f32(light, "intensity")
                .map_err(|error| format!("fragment light[{light_index}] {error}"))?;
            let falloff = required_f32(light, "falloff")
                .map_err(|error| format!("fragment light[{light_index}] {error}"))?;
            let falloff_exponent = required_f32(light, "falloff_exponent")
                .map_err(|error| format!("fragment light[{light_index}] {error}"))?;
            let flags = required_u32(light, "flags")
                .map_err(|error| format!("fragment light[{light_index}] {error}"))?;
            let time_flags = required_u32(light, "time_flags")
                .map_err(|error| format!("fragment light[{light_index}] {error}"))?;
            let cone_inner_degrees = required_f32(light, "cone_inner_degrees")
                .map_err(|error| format!("fragment light[{light_index}] {error}"))?;
            let cone_outer_degrees = required_f32(light, "cone_outer_degrees")
                .map_err(|error| format!("fragment light[{light_index}] {error}"))?;
            let corona_intensity = required_f32(light, "corona_intensity")
                .map_err(|error| format!("fragment light[{light_index}] {error}"))?;
            let volume_intensity = required_f32(light, "volume_intensity")
                .map_err(|error| format!("fragment light[{light_index}] {error}"))?;
            Ok(ModelFragmentLight {
                index,
                part_index,
                bone_index,
                bone_name,
                light_type,
                group_id,
                position,
                direction,
                tangent,
                color,
                intensity,
                falloff,
                falloff_exponent,
                flags,
                time_flags,
                cone_inner_degrees,
                cone_outer_degrees,
                corona_intensity,
                volume_intensity,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;

    for part in &parts {
        if let Some(parent_index) = part.parent_part_index {
            if parent_index == part.index {
                return Err(format!(
                    "fragment part '{}' cannot parent itself",
                    part.name
                ));
            }
            if !parts
                .iter()
                .any(|candidate| candidate.index == parent_index)
            {
                return Err(format!(
                    "fragment part '{}' references missing parent index {}",
                    part.name, parent_index
                ));
            }
        }
    }

    for light in &lights {
        if let Some(part_index) = light.part_index {
            if !parts.iter().any(|part| part.index == part_index) {
                return Err(format!(
                    "fragment light {} references missing part index {}",
                    light.index, part_index
                ));
            }
        }
    }

    Ok(Some(ModelFragmentMetadata {
        bound_center,
        bound_radius,
        parts,
        lights,
    }))
}

fn decode_required_mat4(value: &Value, key: &str) -> Result<[f32; 16], String> {
    let values = required(value, key)?
        .as_array()
        .ok_or_else(|| format!("{key} must be a 16-element array"))?;
    if values.len() != 16 {
        return Err(format!("{key} has {} elements, expected 16", values.len()));
    }
    let mut out = [0.0f32; 16];
    for (index, value) in values.iter().enumerate() {
        let value = value
            .as_f64()
            .map(|value| value as f32)
            .filter(|value| value.is_finite())
            .ok_or_else(|| format!("{key}[{index}] must be finite numeric"))?;
        out[index] = value;
    }
    Ok(out)
}

fn decode_optional_mat4(value: Option<&Value>) -> Result<[f32; 16], String> {
    let identity = [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
    let Some(value) = value.filter(|value| !value.is_null()) else {
        return Ok(identity);
    };
    let values = value
        .as_array()
        .ok_or_else(|| "skin_source_to_model must be a 16-element array".to_owned())?;
    if values.len() != 16 {
        return Err(format!(
            "skin_source_to_model has {} elements, expected 16",
            values.len()
        ));
    }
    let mut out = [0.0; 16];
    for (index, value) in values.iter().enumerate() {
        let value = value
            .as_f64()
            .ok_or_else(|| format!("skin_source_to_model[{index}] must be numeric"))?
            as f32;
        if !value.is_finite() {
            return Err(format!("skin_source_to_model[{index}] is non-finite"));
        }
        out[index] = value;
    }
    Ok(out)
}

fn build_builtin_material_resource(
    model_address: &AssetAddress,
    slot_index: usize,
    name: &str,
    slot: &Value,
    surface_domain: &str,
    blend_mode: &str,
    textures: &[ImportedTextureBinding],
    shading: ImportedMaterialShading,
) -> Result<MaterialResource, String> {
    let model_id = AssetId::from_address(model_address).0;
    let slot_id = (slot_index as u64)
        .wrapping_add(1)
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .rotate_left(17);
    let material_id = AssetId(model_id ^ slot_id);

    if let Some(inline_material) = slot.get("inline_material") {
        return material::decode_inline_material_resource(material_id, inline_material);
    }

    let shader = slot
        .get("shader")
        .or_else(|| slot.get("shader_name"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("pbr.default")
        .to_owned();
    let blend = decode_builtin_blend_mode(blend_mode)?;
    let two_sided = slot
        .get("two_sided")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let texture_bindings = textures
        .iter()
        .map(|binding| MaterialTextureBinding {
            slot: binding.role.clone(),
            texture: binding.texture.clone(),
            texture_name: Some(binding.texture_name.clone()),
            required: binding.required,
        })
        .collect::<Vec<_>>();
    let mut params = [
        ("normal_strength", shading.normal_strength),
        ("specular_intensity", shading.specular_intensity),
        ("specular_falloff", shading.specular_falloff),
        ("specular_fresnel", shading.specular_fresnel),
        ("emissive_multiplier", shading.emissive_multiplier),
        ("environment_reflection", shading.environment_reflection),
        ("alpha_cutoff", shading.alpha_cutoff),
        ("opacity", shading.opacity),
    ]
    .into_iter()
    .map(|(name, value)| MaterialParameter {
        name: name.to_owned(),
        value: MaterialParamValue::Float(value),
    })
    .collect::<Vec<_>>();
    if let Some(render_bucket) = slot
        .get("source_shader")
        .and_then(|source| source.get("render_bucket"))
        .and_then(Value::as_u64)
        .and_then(|value| i32::try_from(value).ok())
    {
        params.push(MaterialParameter {
            name: "render_bucket".to_owned(),
            value: MaterialParamValue::Int(render_bucket),
        });
    }

    Ok(MaterialResource {
        id: material_id,
        name: name.to_owned(),
        shader,
        surface_domain: surface_domain.to_owned(),
        shading_model: "pbr".to_owned(),
        blend,
        two_sided,
        alpha_cutoff: Some(shading.alpha_cutoff),
        textures: texture_bindings,
        params,
    })
}

fn decode_builtin_blend_mode(value: &str) -> Result<BlendMode, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "opaque" => Ok(BlendMode::Opaque),
        "masked" | "mask" | "cutout" => Ok(BlendMode::Masked),
        "alpha" | "blend" | "transparent" | "translucent" => Ok(BlendMode::Alpha),
        "additive" | "add" => Ok(BlendMode::Additive),
        other => Err(format!(
            "unsupported built-in material blend mode '{other}'"
        )),
    }
}

fn decode_material_shading(
    slot: &Value,
    parameters: &[ImportedMaterialParameter],
) -> ImportedMaterialShading {
    const BUMPINESS: u32 = 4_134_611_841;
    const SPECULAR_INTENSITY_MULT: u32 = 4_095_226_703;
    const SPECULAR_FALLOFF_MULT: u32 = 2_272_544_384;
    const SPECULAR_FRESNEL: u32 = 666_481_402;
    const EMISSIVE_MULTIPLIER: u32 = 1_592_520_008;

    let legacy_scalar = |hash: u32, default: f32| {
        parameters
            .iter()
            .find(|parameter| parameter.source_parameter_hash == hash)
            .and_then(|parameter| parameter.values.first())
            .map(|value| value[0])
            .filter(|value| value.is_finite())
            .unwrap_or(default)
    };
    let field = |key: &str, default: f32| {
        slot.get("shading")
            .and_then(|value| value.get(key))
            .and_then(Value::as_f64)
            .map(|value| value as f32)
            .filter(|value| value.is_finite())
            .unwrap_or(default)
    };

    ImportedMaterialShading {
        normal_strength: field("normal_strength", legacy_scalar(BUMPINESS, 1.0)).max(0.0),
        specular_intensity: field(
            "specular_intensity",
            legacy_scalar(SPECULAR_INTENSITY_MULT, 0.0),
        )
        .max(0.0),
        specular_falloff: field(
            "specular_falloff",
            legacy_scalar(SPECULAR_FALLOFF_MULT, 32.0),
        )
        .clamp(1.0, 512.0),
        specular_fresnel: field("specular_fresnel", legacy_scalar(SPECULAR_FRESNEL, 0.0))
            .clamp(0.0, 1.0),
        emissive_multiplier: field(
            "emissive_multiplier",
            legacy_scalar(EMISSIVE_MULTIPLIER, 0.0),
        )
        .max(0.0),
        environment_reflection: field(
            "environment_reflection",
            rsc7_legacy_environment_reflection(slot),
        )
        .max(0.0),
        alpha_cutoff: field("alpha_cutoff", 0.5).clamp(0.0, 1.0),
        opacity: field("opacity", rsc7_legacy_opacity(slot)).clamp(0.0, 1.0),
    }
}

fn rsc7_legacy_environment_reflection(slot: &Value) -> f32 {
    let shader_hash = slot
        .get("source_shader")
        .and_then(|source| source.get("name_hash"))
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or(0);
    match shader_hash {
        // RSC7 glass families sample the scene/global environment even when
        // no material-local EnvironmentSampler is serialized.
        2_242_617_867 | 3_305_737_954 | 3_061_885_788 | 3_656_960_447 | 445_713_287
        | 4_163_455_306 => 1.0,
        _ => 0.0,
    }
}

fn rsc7_legacy_opacity(slot: &Value) -> f32 {
    let shader_hash = slot
        .get("source_shader")
        .and_then(|source| source.get("name_hash"))
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or(0);
    match shader_hash {
        // glass_env
        2_242_617_867 => 0.35,
        // glass_pv_env (typically heavily tinted / ~10 percent glass)
        3_305_737_954 => 0.18,
        // glass_reflect
        3_061_885_788 => 0.28,
        // glass
        3_656_960_447 => 0.35,
        // glass_emissive
        445_713_287 => 0.45,
        // normal_reflect glassware-like family
        4_163_455_306 => 0.42,
        // Other bucket-1 families can carry authored texture alpha.
        _ => 1.0,
    }
}

fn decode_collision_resource(
    address: &AssetAddress,
    meta: &Value,
    payload: &[u8],
) -> Result<CollisionMeshResource, String> {
    require_schema(meta, "northstar.collision.runtime.v1")?;
    let name = required_str(meta, "name")?.to_owned();
    let bounds_value = required(meta, "bounds")?;
    let bounds3 = decode_bounds(bounds_value)?;
    let vertex_count = required_usize(meta, "vertex_count")?;
    let triangle_count = required_usize(meta, "triangle_count")?;

    let vertex_bytes = payload_slice(payload, required(meta, "vertices")?, "vertices")?;
    let expected_vertex_bytes = vertex_count
        .checked_mul(12)
        .ok_or_else(|| "collision vertex byte size overflow".to_owned())?;
    if vertex_bytes.len() != expected_vertex_bytes {
        return Err(format!(
            "collision vertex bytes={} expected={expected_vertex_bytes}",
            vertex_bytes.len()
        ));
    }
    let vertices = vertex_bytes
        .chunks_exact(12)
        .map(|bytes| {
            [
                f32::from_le_bytes(bytes[0..4].try_into().unwrap()),
                f32::from_le_bytes(bytes[4..8].try_into().unwrap()),
                f32::from_le_bytes(bytes[8..12].try_into().unwrap()),
            ]
        })
        .collect::<Vec<_>>();

    let triangle_bytes = payload_slice(payload, required(meta, "triangles")?, "triangles")?;
    let expected_triangle_bytes = triangle_count
        .checked_mul(12)
        .ok_or_else(|| "collision triangle byte size overflow".to_owned())?;
    if triangle_bytes.len() != expected_triangle_bytes {
        return Err(format!(
            "collision triangle bytes={} expected={expected_triangle_bytes}",
            triangle_bytes.len()
        ));
    }
    let triangles = triangle_bytes
        .chunks_exact(12)
        .map(|bytes| {
            [
                u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
                u32::from_le_bytes(bytes[4..8].try_into().unwrap()),
                u32::from_le_bytes(bytes[8..12].try_into().unwrap()),
            ]
        })
        .collect::<Vec<_>>();

    let material_bytes = payload_slice(payload, required(meta, "materials")?, "materials")?;
    if material_bytes.len() % 4 != 0 {
        return Err("collision material payload is not u32-aligned".to_owned());
    }
    let material_indices = material_bytes
        .chunks_exact(4)
        .map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap()))
        .collect::<Vec<_>>();

    let resource = CollisionMeshResource {
        id: AssetId::from_address(address),
        name,
        vertices,
        triangles,
        material_indices,
        bounds: CollisionBounds {
            min: bounds3.min,
            max: bounds3.max,
        },
    };
    resource.validate()?;
    Ok(resource)
}

fn payload_slice<'a>(payload: &'a [u8], meta: &Value, label: &str) -> Result<&'a [u8], String> {
    let offset = required_usize(meta, "offset")?;
    let length = required_usize(meta, "length")?;
    let end = offset
        .checked_add(length)
        .ok_or_else(|| format!("{label} payload range overflow"))?;
    payload.get(offset..end).ok_or_else(|| {
        format!(
            "{label} payload range {offset}..{end} exceeds {} bytes",
            payload.len()
        )
    })
}

fn parse_semantic(value: &str) -> Result<VertexSemantic, String> {
    match value {
        "position" => Ok(VertexSemantic::Position),
        "normal" => Ok(VertexSemantic::Normal),
        "tangent" => Ok(VertexSemantic::Tangent),
        "texcoord0" => Ok(VertexSemantic::TexCoord(0)),
        "color0" => Ok(VertexSemantic::Color(0)),
        "joint_indices" => Ok(VertexSemantic::JointIndices),
        "joint_weights" => Ok(VertexSemantic::JointWeights),
        "joint_indices_extra" => Ok(VertexSemantic::JointIndicesExtra),
        "joint_weights_extra" => Ok(VertexSemantic::JointWeightsExtra),
        other if !other.trim().is_empty() => Ok(VertexSemantic::Custom(other.to_owned())),
        _ => Err("empty vertex semantic".to_owned()),
    }
}

fn parse_vertex_format(value: &str) -> Result<VertexFormat, String> {
    match value {
        "f32x2" => Ok(VertexFormat::Float32x2),
        "f32x3" => Ok(VertexFormat::Float32x3),
        "f32x4" => Ok(VertexFormat::Float32x4),
        other => Err(format!("unsupported vertex format '{other}'")),
    }
}

fn decode_bounds(value: &Value) -> Result<Bounds3, String> {
    let min = required_vec3(value, "min")?;
    let max = required_vec3(value, "max")?;
    let bounds = Bounds3 { min, max };
    if !bounds.is_finite() {
        return Err("bounds contain non-finite values".to_owned());
    }
    Ok(bounds)
}

fn require_schema(value: &Value, expected: &str) -> Result<(), String> {
    let actual = required_str(value, "schema")?;
    if actual != expected {
        return Err(format!(
            "semantic schema mismatch: actual='{actual}' expected='{expected}'"
        ));
    }
    Ok(())
}

fn required<'a>(value: &'a Value, key: &str) -> Result<&'a Value, String> {
    value
        .get(key)
        .ok_or_else(|| format!("missing field '{key}'"))
}

fn required_str<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    required(value, key)?
        .as_str()
        .ok_or_else(|| format!("field '{key}' must be a string"))
}

fn required_usize(value: &Value, key: &str) -> Result<usize, String> {
    let raw = required(value, key)?
        .as_u64()
        .ok_or_else(|| format!("field '{key}' must be an unsigned integer"))?;
    usize::try_from(raw).map_err(|_| format!("field '{key}' exceeds usize"))
}

fn required_u32(value: &Value, key: &str) -> Result<u32, String> {
    let raw = required(value, key)?
        .as_u64()
        .ok_or_else(|| format!("field '{key}' must be an unsigned integer"))?;
    u32::try_from(raw).map_err(|_| format!("field '{key}' exceeds u32"))
}

fn required_u16(value: &Value, key: &str) -> Result<u16, String> {
    let raw = required(value, key)?
        .as_u64()
        .ok_or_else(|| format!("field '{key}' must be an unsigned integer"))?;
    u16::try_from(raw).map_err(|_| format!("field '{key}' exceeds u16"))
}

fn required_u8(value: &Value, key: &str) -> Result<u8, String> {
    let raw = required(value, key)?
        .as_u64()
        .ok_or_else(|| format!("field '{key}' must be an unsigned integer"))?;
    u8::try_from(raw).map_err(|_| format!("field '{key}' exceeds u8"))
}

fn required_f32(value: &Value, key: &str) -> Result<f32, String> {
    required(value, key)?
        .as_f64()
        .map(|value| value as f32)
        .filter(|value| value.is_finite())
        .ok_or_else(|| format!("field '{key}' must be finite numeric"))
}

fn required_i32(value: &Value, key: &str) -> Result<i32, String> {
    let raw = required(value, key)?
        .as_i64()
        .ok_or_else(|| format!("field '{key}' must be an integer"))?;
    i32::try_from(raw).map_err(|_| format!("field '{key}' exceeds i32"))
}

fn required_vec4(value: &Value, key: &str) -> Result<[f32; 4], String> {
    let array = required(value, key)?
        .as_array()
        .ok_or_else(|| format!("field '{key}' must be vec4"))?;
    if array.len() != 4 {
        return Err(format!("field '{key}' must contain exactly 4 numbers"));
    }
    let mut out = [0.0f32; 4];
    for (index, target) in out.iter_mut().enumerate() {
        *target = array[index]
            .as_f64()
            .map(|value| value as f32)
            .filter(|value| value.is_finite())
            .ok_or_else(|| format!("field '{key}[{index}]' must be finite"))?;
    }
    Ok(out)
}

fn required_vec3(value: &Value, key: &str) -> Result<[f32; 3], String> {
    let array = required(value, key)?
        .as_array()
        .ok_or_else(|| format!("field '{key}' must be vec3"))?;
    if array.len() != 3 {
        return Err(format!("field '{key}' must contain exactly 3 numbers"));
    }
    let mut out = [0.0f32; 3];
    for (index, target) in out.iter_mut().enumerate() {
        *target = array[index]
            .as_f64()
            .map(|value| value as f32)
            .filter(|value| value.is_finite())
            .ok_or_else(|| format!("field '{key}[{index}]' must be finite"))?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fragment_authored_light_survives_semantic_decode() {
        let value = json!({
            "schema": "northstar.model.fragment.v1",
            "bound_center": [0.0, 0.5, -1.0],
            "bound_radius": 3.0,
            "parts": [{
                "index": 7,
                "name": "headlight_l",
                "role": "light",
                "bone_tag": 111,
                "group_index": null,
                "parent_index": null,
                "wheel_slot": null,
                "mesh_names": [],
                "rest_transform": [
                    1.0,0.0,0.0,0.0,
                    0.0,1.0,0.0,0.0,
                    0.0,0.0,1.0,0.0,
                    0.0,0.0,0.0,1.0
                ],
                "rest_position": [0.0, 0.0, 0.0]
            }],
            "lights": [{
                "index": 2,
                "part_index": 7,
                "bone_index": 31,
                "bone_name": "headlight_l",
                "light_type": 2,
                "group_id": 1,
                "position": [-0.7, 0.8, 2.1],
                "direction": [0.0, -0.1, 1.0],
                "tangent": [1.0, 0.0, 0.0],
                "color": [1.0, 0.8, 0.5],
                "intensity": 7.5,
                "falloff": 42.0,
                "falloff_exponent": 2.0,
                "flags": 5,
                "time_flags": 255,
                "cone_inner_degrees": 18.0,
                "cone_outer_degrees": 30.0,
                "corona_intensity": 0.6,
                "volume_intensity": 0.25
            }]
        });
        let fragment = decode_optional_fragment(Some(&value), &[])
            .expect("decode fragment")
            .expect("fragment");
        assert_eq!(fragment.lights.len(), 1);
        let light = &fragment.lights[0];
        assert_eq!(light.index, 2);
        assert_eq!(light.part_index, Some(7));
        assert_eq!(light.bone_index, 31);
        assert_eq!(light.bone_name.as_deref(), Some("headlight_l"));
        assert_eq!(light.light_type, 2);
        assert_eq!(light.position, [-0.7, 0.8, 2.1]);
        assert!((light.intensity - 7.5).abs() < 1.0e-6);
        assert!((light.cone_outer_degrees - 30.0).abs() < 1.0e-6);
    }

    #[test]
    fn fragment_without_authored_lights_remains_backward_compatible() {
        let value = json!({
            "schema": "northstar.model.fragment.v1",
            "bound_center": [0.0, 0.0, 0.0],
            "bound_radius": 1.0,
            "parts": []
        });
        let fragment = decode_optional_fragment(Some(&value), &[])
            .expect("decode fragment")
            .expect("fragment");
        assert!(fragment.lights.is_empty());
    }

    #[test]
    fn rsc7_ytd_texture_selectors_are_canonicalized_to_lowercase() {
        let address =
            AssetAddress::parse("textures/weapons/gta/w_ar_carbinerifle.ytd@W_AR_CarbineRifle")
                .unwrap();
        let normalized = normalize_rsc7_texture_address(address);
        assert_eq!(
            normalized.canonical(),
            "textures/weapons/gta/w_ar_carbinerifle.ytd@w_ar_carbinerifle"
        );

        let non_ytd = AssetAddress::parse("materials/example.ymat@MiXeD").unwrap();
        assert_eq!(
            normalize_rsc7_texture_address(non_ytd).canonical(),
            "materials/example.ymat@MiXeD"
        );
    }

    #[test]
    fn unresolved_optional_builtin_texture_does_not_become_required() {
        let address = AssetAddress::parse("models/test.ydr").unwrap();
        let material = build_builtin_material_resource(
            &address,
            0,
            "material.0",
            &json!({}),
            "surface",
            "opaque",
            &[ImportedTextureBinding {
                role: "generic".to_owned(),
                texture_name: "rsc7_fallback_sampler".to_owned(),
                texture: None,
                required: false,
            }],
            decode_material_shading(&json!({}), &[]),
        )
        .unwrap();
        assert_eq!(material.textures.len(), 1);
        assert!(!material.textures[0].required);
        assert!(material.textures[0].direct_address().is_none());
        assert!(material.dependencies().is_empty());
    }

    #[test]
    fn descriptor_support_is_semantic_not_extension_based() {
        let type_info = json!({
            "known": true,
            "descriptor": {
                "outputs": ["model.runtime_v1"]
            }
        });
        assert!(descriptor_supports(&type_info, MODEL_RUNTIME_OUTPUT));
        assert!(!descriptor_supports(&type_info, COLLISION_RUNTIME_OUTPUT));
    }

    #[test]
    fn semantic_wire_rejects_wrong_magic() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"NOPE");
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&2u32.to_le_bytes());
        bytes.extend_from_slice(&0u64.to_le_bytes());
        bytes.extend_from_slice(b"{}");
        assert!(unpack_semantic_wire(&bytes, MODEL_WIRE_MAGIC).is_err());
    }

    #[test]
    fn legacy_rsc7_parameters_are_normalized_to_shading() {
        let parameters = vec![
            ImportedMaterialParameter {
                source_parameter_hash: 4_134_611_841,
                values: vec![[0.6, 0.0, 0.0, 0.0]],
            },
            ImportedMaterialParameter {
                source_parameter_hash: 4_095_226_703,
                values: vec![[0.26, 0.0, 0.0, 0.0]],
            },
            ImportedMaterialParameter {
                source_parameter_hash: 2_272_544_384,
                values: vec![[80.0, 0.0, 0.0, 0.0]],
            },
            ImportedMaterialParameter {
                source_parameter_hash: 666_481_402,
                values: vec![[0.9, 0.0, 0.0, 0.0]],
            },
            ImportedMaterialParameter {
                source_parameter_hash: 1_592_520_008,
                values: vec![[3.0, 0.0, 0.0, 0.0]],
            },
        ];
        let shading = decode_material_shading(&json!({}), &parameters);
        assert!((shading.normal_strength - 0.6).abs() < 1.0e-6);
        assert!((shading.specular_intensity - 0.26).abs() < 1.0e-6);
        assert!((shading.specular_falloff - 80.0).abs() < 1.0e-6);
        assert!((shading.specular_fresnel - 0.9).abs() < 1.0e-6);
        assert!((shading.emissive_multiplier - 3.0).abs() < 1.0e-6);
    }

    #[test]
    fn legacy_glass_family_gets_translucent_opacity() {
        let slot = json!({
            "source_shader": {
                "name_hash": 2_242_617_867u64,
                "file_hash": 1_263_059_426u64,
                "render_bucket": 1
            }
        });
        let shading = decode_material_shading(&slot, &[]);
        assert!((shading.opacity - 0.35).abs() < 1.0e-6);
        assert!((shading.environment_reflection - 1.0).abs() < 1.0e-6);
    }

    #[test]
    fn non_glass_material_does_not_request_global_environment_reflection() {
        let slot = json!({
            "source_shader": {
                "name_hash": 954_007_775u64,
                "file_hash": 346_521_853u64,
                "render_bucket": 0
            }
        });
        let shading = decode_material_shading(&slot, &[]);
        assert_eq!(shading.environment_reflection, 0.0);
    }

    #[test]
    fn explicit_semantic_shading_overrides_legacy_values() {
        let parameters = vec![ImportedMaterialParameter {
            source_parameter_hash: 4_134_611_841,
            values: vec![[0.25, 0.0, 0.0, 0.0]],
        }];
        let slot = json!({
            "shading": {
                "normal_strength": 1.5,
                "specular_intensity": 0.75,
                "specular_falloff": 120.0,
                "specular_fresnel": 0.5,
                "emissive_multiplier": 2.0,
                "environment_reflection": 0.65,
                "alpha_cutoff": 0.33,
                "opacity": 0.42
            }
        });
        let shading = decode_material_shading(&slot, &parameters);
        assert!((shading.normal_strength - 1.5).abs() < 1.0e-6);
        assert!((shading.specular_intensity - 0.75).abs() < 1.0e-6);
        assert!((shading.specular_falloff - 120.0).abs() < 1.0e-6);
        assert!((shading.specular_fresnel - 0.5).abs() < 1.0e-6);
        assert!((shading.emissive_multiplier - 2.0).abs() < 1.0e-6);
        assert!((shading.environment_reflection - 0.65).abs() < 1.0e-6);
        assert!((shading.alpha_cutoff - 0.33).abs() < 1.0e-6);
        assert!((shading.opacity - 0.42).abs() < 1.0e-6);
    }
}
