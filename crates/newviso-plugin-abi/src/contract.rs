pub use newengine_contract_api::{ContractCompatibility, ContractKind, ContractVersion};

use crate::{CapabilityDesc, CapabilityKind, CapabilityRole};

/// Reserved descriptor capability namespace for contracts contributed to the
/// instance-scoped Runtime Contract Catalog. The declaration is metadata only;
/// it does not create a service or mutate the normative Engine Contract Registry.
pub const RUNTIME_CONTRACT_CAPABILITY_PREFIX: &str = "runtime.contract.";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeContractDeclaration {
    pub key: String,
    pub kind: ContractKind,
    pub version: ContractVersion,
    pub compatibility: ContractCompatibility,
    pub advertised_id: Option<String>,
}

impl RuntimeContractDeclaration {
    pub fn new(
        key: impl Into<String>,
        kind: ContractKind,
        version: ContractVersion,
        compatibility: ContractCompatibility,
    ) -> Self {
        Self {
            key: key.into(),
            kind,
            version,
            compatibility,
            advertised_id: None,
        }
    }

    #[inline]
    pub fn advertised_id(mut self, id: impl Into<String>) -> Self {
        self.advertised_id = Some(id.into());
        self
    }

    /// Encodes this declaration into the existing extensible plugin descriptor
    /// surface, avoiding an ABI change to PluginDescriptor/PluginDescriptorV2.
    pub fn into_capability(self) -> CapabilityDesc {
        let key = self.key.trim().to_owned();
        let id = format!("{RUNTIME_CONTRACT_CAPABILITY_PREFIX}{key}");
        let json = serde_json::json!({
            "runtime_contract": {
                "kind": contract_kind_name(self.kind),
                "version": {
                    "major": self.version.major,
                    "minor": self.version.minor,
                    "patch": self.version.patch,
                },
                "compatibility": compatibility_name(self.compatibility),
                "advertised_id": self.advertised_id,
            }
        });
        CapabilityDesc::new(id, CapabilityRole::Provides, CapabilityKind::Other, 1)
            .with_json(json.to_string())
    }
}

/// Parses a runtime-contract declaration from one capability. `Ok(None)` means
/// the capability belongs to another namespace. Generic host code should use this
/// helper rather than interpreting declaration JSON itself.
pub fn runtime_contract_declaration(
    capability: &CapabilityDesc,
) -> Result<Option<RuntimeContractDeclaration>, String> {
    if capability.role != CapabilityRole::Provides || capability.kind != CapabilityKind::Other {
        return Ok(None);
    }
    let Some(key) = capability
        .id
        .as_str()
        .strip_prefix(RUNTIME_CONTRACT_CAPABILITY_PREFIX)
    else {
        return Ok(None);
    };
    let key = key.trim();
    if key.is_empty() {
        return Err("runtime contract capability has empty key".to_owned());
    }
    let value: serde_json::Value = serde_json::from_str(capability.describe_json.as_str())
        .map_err(|error| format!("runtime contract '{key}' metadata is invalid JSON: {error}"))?;
    let document = value
        .get("runtime_contract")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| format!("runtime contract '{key}' is missing runtime_contract metadata"))?;
    let kind = parse_kind(
        document
            .get("kind")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| format!("runtime contract '{key}' is missing kind"))?,
    )?;
    let version = document
        .get("version")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| format!("runtime contract '{key}' is missing version"))?;
    let part = |name: &str| -> Result<u16, String> {
        let raw = version
            .get(name)
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| format!("runtime contract '{key}' has invalid version.{name}"))?;
        u16::try_from(raw)
            .map_err(|_| format!("runtime contract '{key}' version.{name} exceeds u16"))
    };
    let compatibility = parse_compatibility(
        document
            .get("compatibility")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| format!("runtime contract '{key}' is missing compatibility"))?,
    )?;
    let advertised_id = document
        .get("advertised_id")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);

    Ok(Some(RuntimeContractDeclaration {
        key: key.to_owned(),
        kind,
        version: ContractVersion::new(part("major")?, part("minor")?, part("patch")?),
        compatibility,
        advertised_id,
    }))
}

fn contract_kind_name(kind: ContractKind) -> &'static str {
    kind.as_str()
}

fn compatibility_name(compatibility: ContractCompatibility) -> &'static str {
    match compatibility {
        ContractCompatibility::Exact => "exact",
        ContractCompatibility::SameMajor => "same_major",
        ContractCompatibility::AtLeast => "at_least",
    }
}

fn parse_kind(value: &str) -> Result<ContractKind, String> {
    match value.trim() {
        "wire" => Ok(ContractKind::Wire),
        "schema" => Ok(ContractKind::Schema),
        "abi" => Ok(ContractKind::Abi),
        "protocol" => Ok(ContractKind::Protocol),
        "manifest" => Ok(ContractKind::Manifest),
        other => Err(format!("unknown runtime contract kind '{other}'")),
    }
}

fn parse_compatibility(value: &str) -> Result<ContractCompatibility, String> {
    match value.trim() {
        "exact" => Ok(ContractCompatibility::Exact),
        "same_major" => Ok(ContractCompatibility::SameMajor),
        "at_least" => Ok(ContractCompatibility::AtLeast),
        other => Err(format!("unknown runtime contract compatibility '{other}'")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declaration_round_trips_through_legacy_descriptor_capability() {
        let declared = RuntimeContractDeclaration::new(
            "acme.streaming.protocol",
            ContractKind::Protocol,
            ContractVersion::new(2, 3, 1),
            ContractCompatibility::SameMajor,
        )
        .advertised_id("acme.streaming.v2");
        let capability = declared.clone().into_capability();
        assert_eq!(
            runtime_contract_declaration(&capability).unwrap(),
            Some(declared)
        );
    }
}
