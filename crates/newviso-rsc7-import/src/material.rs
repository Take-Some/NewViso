use newviso_assets_client::AssetClient;
use newviso_materials::{
    BlendMode, MaterialParamValue, MaterialParameter, MaterialResource, MaterialTextureBinding,
};
use newviso_resource_runtime::{AssetAddress, AssetId, AssetRef, AssetResource, ResourceDecoder};
use newviso_textures::TextureResource;
use serde_json::{json, Value};
use std::sync::Arc;

const MATERIAL_RUNTIME_OUTPUT: &str = "material.runtime_v1";
const MATERIAL_RUNTIME_SCHEMA: &str = "northstar.material.runtime.v1";

#[derive(Clone, Copy, Debug, Default)]
pub struct SemanticMaterialDecoder;

impl ResourceDecoder for SemanticMaterialDecoder {
    fn name(&self) -> &'static str {
        "newviso.asset_manager.material_semantic.v1"
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
        let supports = type_info
            .get("descriptor")
            .and_then(|value| value.get("outputs"))
            .and_then(Value::as_array)
            .is_some_and(|outputs| {
                outputs
                    .iter()
                    .any(|candidate| candidate.as_str() == Some(MATERIAL_RUNTIME_OUTPUT))
            });
        if !supports {
            return Ok(None);
        }

        let value = assets.decode_json(
            address.logical_path(),
            MATERIAL_RUNTIME_OUTPUT,
            json!({"entry": entry}),
        )?;
        let resource = decode_material_resource(address, &value)?;
        Ok(Some(Arc::new(resource)))
    }
}

pub(crate) fn decode_inline_material_resource(
    id: AssetId,
    value: &Value,
) -> Result<MaterialResource, String> {
    let mut normalized = value.clone();
    let object = normalized
        .as_object_mut()
        .ok_or_else(|| "inline material semantic value must be an object".to_owned())?;
    object
        .entry("schema".to_owned())
        .or_insert_with(|| Value::String(MATERIAL_RUNTIME_SCHEMA.to_owned()));
    let synthetic = AssetAddress::parse("materials/__inline.ymat@inline")?;
    let mut resource = decode_material_resource(&synthetic, &normalized)?;
    resource.id = id;
    Ok(resource)
}

fn decode_material_resource(
    address: &AssetAddress,
    value: &Value,
) -> Result<MaterialResource, String> {
    if value.get("schema").and_then(Value::as_str) != Some(MATERIAL_RUNTIME_SCHEMA) {
        return Err(format!(
            "material semantic schema mismatch address='{}' actual='{}'",
            address.canonical(),
            value
                .get("schema")
                .and_then(Value::as_str)
                .unwrap_or("<missing>")
        ));
    }

    let name = required_text(value, "name")?.to_owned();
    let shader = required_text(value, "shader")?.to_owned();
    let surface_domain = value
        .get("surface_domain")
        .and_then(Value::as_str)
        .unwrap_or("surface")
        .to_owned();
    let shading_model = value
        .get("shading_model")
        .and_then(Value::as_str)
        .unwrap_or("pbr")
        .to_owned();
    let blend = match value
        .get("blend")
        .and_then(Value::as_str)
        .unwrap_or("opaque")
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "opaque" => BlendMode::Opaque,
        "masked" | "mask" | "cutout" => BlendMode::Masked,
        "alpha" | "blend" | "transparent" => BlendMode::Alpha,
        "additive" | "add" => BlendMode::Additive,
        other => {
            return Err(format!(
                "material '{}' has unsupported blend '{other}'",
                name
            ))
        }
    };
    let two_sided = value
        .get("two_sided")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let alpha_cutoff = value
        .get("alpha_cutoff")
        .and_then(Value::as_f64)
        .map(|value| value as f32)
        .filter(|value| value.is_finite());

    let textures = value
        .get("textures")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .enumerate()
                .map(|(index, item)| {
                    let slot = required_text(item, "slot")
                        .map_err(|error| format!("material '{name}' texture[{index}]: {error}"))?
                        .to_owned();
                    let reference = required_text(item, "ref")
                        .map_err(|error| format!("material '{name}' texture[{index}]: {error}"))?;
                    let texture_address = AssetAddress::parse(reference).map_err(|error| {
                        format!(
                            "material '{name}' texture[{index}] ref '{reference}' invalid: {error}"
                        )
                    })?;
                    Ok(MaterialTextureBinding {
                        slot,
                        texture_name: texture_address.entry().map(str::to_owned),
                        texture: Some(AssetRef::<TextureResource>::new(texture_address)),
                        required: item
                            .get("required")
                            .and_then(Value::as_bool)
                            .unwrap_or(true),
                    })
                })
                .collect::<Result<Vec<_>, String>>()
        })
        .transpose()?
        .unwrap_or_default();

    let params = value
        .get("params")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .enumerate()
                .map(|(index, item)| decode_parameter(&name, index, item))
                .collect::<Result<Vec<_>, String>>()
        })
        .transpose()?
        .unwrap_or_default();

    Ok(MaterialResource {
        id: AssetId::from_address(address),
        name,
        shader,
        surface_domain,
        shading_model,
        blend,
        two_sided,
        alpha_cutoff,
        textures,
        params,
    })
}

fn decode_parameter(
    material_name: &str,
    index: usize,
    value: &Value,
) -> Result<MaterialParameter, String> {
    let name = required_text(value, "name")
        .map_err(|error| format!("material '{material_name}' param[{index}]: {error}"))?
        .to_owned();
    let kind = required_text(value, "type")
        .map_err(|error| format!("material '{material_name}' param[{index}]: {error}"))?
        .to_ascii_lowercase();
    let raw = value
        .get("value")
        .ok_or_else(|| format!("material '{material_name}' param[{index}] missing 'value'"))?;

    let value = match kind.as_str() {
        "float" => MaterialParamValue::Float(number(raw, material_name, index)?),
        "float2" => MaterialParamValue::Float2(vector::<2>(raw, material_name, index)?),
        "float3" => MaterialParamValue::Float3(vector::<3>(raw, material_name, index)?),
        "float4" | "color" => MaterialParamValue::Float4(vector::<4>(raw, material_name, index)?),
        "int" => {
            let number = raw
                .as_i64()
                .ok_or_else(|| format!("material '{material_name}' param[{index}] must be int"))?;
            MaterialParamValue::Int(
                i32::try_from(number).map_err(|_| {
                    format!("material '{material_name}' param[{index}] exceeds i32")
                })?,
            )
        }
        "bool" => MaterialParamValue::Bool(
            raw.as_bool()
                .ok_or_else(|| format!("material '{material_name}' param[{index}] must be bool"))?,
        ),
        "enum" => MaterialParamValue::Enum(
            raw.as_str()
                .ok_or_else(|| format!("material '{material_name}' param[{index}] must be string"))?
                .to_owned(),
        ),
        "texture" | "texture_ref" => {
            let reference = raw.as_str().ok_or_else(|| {
                format!("material '{material_name}' param[{index}] texture ref must be string")
            })?;
            MaterialParamValue::TextureRef(AssetRef::new(AssetAddress::parse(reference)?))
        }
        other => {
            return Err(format!(
                "material '{material_name}' param[{index}] has unsupported type '{other}'"
            ))
        }
    };

    Ok(MaterialParameter { name, value })
}

fn number(value: &Value, material_name: &str, index: usize) -> Result<f32, String> {
    value
        .as_f64()
        .map(|value| value as f32)
        .filter(|value| value.is_finite())
        .ok_or_else(|| format!("material '{material_name}' param[{index}] must be finite number"))
}

fn vector<const N: usize>(
    value: &Value,
    material_name: &str,
    index: usize,
) -> Result<[f32; N], String> {
    let values = value
        .as_array()
        .ok_or_else(|| format!("material '{material_name}' param[{index}] must be array[{N}]"))?;
    if values.len() != N {
        return Err(format!(
            "material '{material_name}' param[{index}] expected {N} components, got {}",
            values.len()
        ));
    }
    let mut out = [0.0; N];
    for (slot, value) in out.iter_mut().zip(values) {
        *slot = value
            .as_f64()
            .map(|value| value as f32)
            .filter(|value| value.is_finite())
            .ok_or_else(|| {
                format!("material '{material_name}' param[{index}] has non-finite component")
            })?;
    }
    Ok(out)
}

fn required_text<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("missing non-empty string '{key}'"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_runtime_material() {
        let address = AssetAddress::parse("materials/test.ymat@m00").unwrap();
        let value = json!({
            "schema": MATERIAL_RUNTIME_SCHEMA,
            "name": "m00",
            "shader": "pbr.default",
            "blend": "masked",
            "two_sided": true,
            "alpha_cutoff": 0.5,
            "textures": [
                {"slot":"base_color","ref":"textures/test.ytd@m00_base","required":true},
                {"slot":"normal","ref":"textures/test.ytd@m00_normal","required":true}
            ],
            "params": [
                {"name":"roughness","type":"float","value":0.62}
            ]
        });
        let material = decode_material_resource(&address, &value).unwrap();
        assert_eq!(material.name, "m00");
        assert_eq!(material.blend, BlendMode::Masked);
        assert_eq!(material.textures.len(), 2);
        assert_eq!(material.params.len(), 1);
    }
}
