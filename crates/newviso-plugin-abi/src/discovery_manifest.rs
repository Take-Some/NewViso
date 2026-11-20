#![forbid(unsafe_op_in_unsafe_fn)]

use abi_stable::std_types::{ROption, RString, RVec};
use serde::{Deserialize, Serialize};

use crate::{
    BackendRouteDescriptorV2, CapabilityDescV2, CapabilityKind, CapabilityRequirementDescV2,
    CapabilityRole, ContractRefV2, PluginBootstrapPhase, PluginDescriptorV2, PluginKind,
    SystemTagV2,
};

pub const PLUGIN_DISCOVERY_MANIFEST_SCHEMA_VERSION: u32 = 2;
pub const PLUGIN_DISCOVERY_MANIFEST_SUFFIX: &str = "nspmeta.json";
/// Fixed trailer marker for metadata embedded directly into a plugin artifact.
pub const PLUGIN_DISCOVERY_EMBEDDED_MAGIC: &[u8; 16] = b"NSPMETA2-EMBED\0\0";
pub const PLUGIN_DISCOVERY_EMBEDDED_FOOTER_SIZE: u64 = 24;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Declarative plugin metadata shipped with the plugin source/artifact.
///
/// Artifact size and SHA-256 are intentionally not serialized here. They are
/// observed by the runtime scanner and frozen in-memory for TOCTOU protection.
pub struct PluginDiscoveryManifestV2 {
    pub schema_version: u32,
    pub artifact_file: String,
    pub signature: Option<PluginDiscoverySignatureV1>,
    pub descriptor: Option<PluginDiscoveryDescriptorV1>,
    pub platform_runtime: Option<PluginDiscoveryPlatformRuntimeV1>,
    pub has_canonical_root: bool,
    pub has_legacy_root: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginDiscoverySignatureV1 {
    pub id: String,
    pub name: String,
    pub version: String,
    pub kind: u8,
    pub bootstrap_phase: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginDiscoveryPlatformRuntimeV1 {
    pub id: String,
    pub name: String,
    pub version: String,
    pub backend_priority: i32,
    pub system_tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginDiscoveryDescriptorV1 {
    pub id: String,
    pub name: String,
    pub version: String,
    pub kind: u8,
    pub capabilities: Vec<PluginDiscoveryCapabilityV1>,
    #[serde(default)]
    pub extension_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginDiscoveryCapabilityV1 {
    pub id: String,
    pub role: u8,
    pub kind: u8,
    pub version: u32,
    pub contract_id: Option<String>,
    pub contract_version: Option<u32>,
    pub tags: Vec<String>,
    pub route: Option<PluginDiscoveryRouteV1>,
    pub requirement: Option<PluginDiscoveryRequirementV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginDiscoveryRouteV1 {
    pub service_kind: String,
    pub engine_gateway: String,
    pub provider_service_id: String,
    pub provider_abi: Option<String>,
    pub provider_route: Option<String>,
    pub backend_priority: i32,
    pub backend: Option<String>,
    pub mode: Option<String>,
    pub features: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginDiscoveryRequirementV1 {
    pub min_version: u32,
    pub max_version: Option<u32>,
    pub required_tags: Vec<String>,
    pub preferred_tags: Vec<String>,
    pub forbidden_tags: Vec<String>,
}

impl PluginDiscoveryDescriptorV1 {
    pub fn from_descriptor_v2(d: &PluginDescriptorV2) -> Self {
        Self {
            id: d.id.to_string(),
            name: d.name.to_string(),
            version: d.version.to_string(),
            kind: plugin_kind_to_u8(d.kind),
            capabilities: d
                .capabilities
                .iter()
                .map(PluginDiscoveryCapabilityV1::from_typed)
                .collect(),
            extension_json: d.extension_json.to_string(),
        }
    }

    pub fn to_descriptor_v2(&self) -> Result<PluginDescriptorV2, String> {
        Ok(PluginDescriptorV2 {
            id: self.id.clone().into(),
            name: self.name.clone().into(),
            version: self.version.clone().into(),
            kind: plugin_kind_from_u8(self.kind)?,
            capabilities: self
                .capabilities
                .iter()
                .map(PluginDiscoveryCapabilityV1::to_typed)
                .collect::<Result<Vec<_>, _>>()?
                .into(),
            extension_json: self.extension_json.clone().into(),
        })
    }
}

impl PluginDiscoveryCapabilityV1 {
    fn from_typed(c: &CapabilityDescV2) -> Self {
        let (contract_id, contract_version) = match &c.contract {
            ROption::RSome(v) => (Some(v.id.to_string()), v.version.into_option()),
            ROption::RNone => (None, None),
        };
        let route = match &c.route {
            ROption::RSome(r) => Some(PluginDiscoveryRouteV1 {
                service_kind: r.service_kind.to_string(),
                engine_gateway: r.engine_gateway.to_string(),
                provider_service_id: r.provider_service_id.to_string(),
                provider_abi: r.provider_abi.clone().into_option().map(|v| v.to_string()),
                provider_route: r
                    .provider_route
                    .clone()
                    .into_option()
                    .map(|v| v.to_string()),
                backend_priority: r.backend_priority,
                backend: r.backend.clone().into_option().map(|v| v.to_string()),
                mode: r.mode.clone().into_option().map(|v| v.to_string()),
                features: r.features.iter().map(|v| v.to_string()).collect(),
            }),
            ROption::RNone => None,
        };
        let requirement = match &c.requirement {
            ROption::RSome(r) => Some(PluginDiscoveryRequirementV1 {
                min_version: r.min_version,
                max_version: r.max_version.into_option(),
                required_tags: r
                    .required_tags
                    .iter()
                    .map(|v| v.as_str().to_owned())
                    .collect(),
                preferred_tags: r
                    .preferred_tags
                    .iter()
                    .map(|v| v.as_str().to_owned())
                    .collect(),
                forbidden_tags: r
                    .forbidden_tags
                    .iter()
                    .map(|v| v.as_str().to_owned())
                    .collect(),
            }),
            ROption::RNone => None,
        };
        Self {
            id: c.id.to_string(),
            role: role_to_u8(c.role),
            kind: cap_kind_to_u8(c.kind),
            version: c.version,
            contract_id,
            contract_version,
            tags: c.tags.iter().map(|v| v.as_str().to_owned()).collect(),
            route,
            requirement,
        }
    }

    fn to_typed(&self) -> Result<CapabilityDescV2, String> {
        let contract = self.contract_id.as_ref().map(|id| ContractRefV2 {
            id: id.clone().into(),
            version: self.contract_version.into(),
        });
        let route = self.route.as_ref().map(|r| BackendRouteDescriptorV2 {
            service_kind: r.service_kind.clone().into(),
            engine_gateway: r.engine_gateway.clone().into(),
            provider_service_id: r.provider_service_id.clone().into(),
            provider_abi: r.provider_abi.clone().map(Into::into).into(),
            provider_route: r.provider_route.clone().map(Into::into).into(),
            backend_priority: r.backend_priority,
            backend: r.backend.clone().map(Into::into).into(),
            mode: r.mode.clone().map(Into::into).into(),
            features: r
                .features
                .iter()
                .cloned()
                .map(RString::from)
                .collect::<Vec<_>>()
                .into(),
        });
        let requirement = self
            .requirement
            .as_ref()
            .map(|r| CapabilityRequirementDescV2 {
                min_version: r.min_version,
                max_version: r.max_version.into(),
                required_tags: tags(&r.required_tags),
                preferred_tags: tags(&r.preferred_tags),
                forbidden_tags: tags(&r.forbidden_tags),
            });
        Ok(CapabilityDescV2 {
            id: self.id.clone().into(),
            role: role_from_u8(self.role)?,
            kind: cap_kind_from_u8(self.kind)?,
            version: self.version,
            contract: contract.into(),
            tags: tags(&self.tags),
            route: route.into(),
            requirement: requirement.into(),
            extension_json: RString::new(),
        })
    }
}

fn tags(v: &[String]) -> RVec<SystemTagV2> {
    v.iter()
        .cloned()
        .map(SystemTagV2::new)
        .collect::<Vec<_>>()
        .into()
}

pub fn plugin_kind_to_u8(v: PluginKind) -> u8 {
    v as u8
}
pub fn bootstrap_phase_to_u8(v: PluginBootstrapPhase) -> u8 {
    v as u8
}
pub fn plugin_kind_from_u8(v: u8) -> Result<PluginKind, String> {
    match v {
        1 => Ok(PluginKind::Runtime),
        2 => Ok(PluginKind::Importer),
        3 => Ok(PluginKind::Editor),
        4 => Ok(PluginKind::Tool),
        255 => Ok(PluginKind::Other),
        _ => Err(format!("invalid plugin kind {v}")),
    }
}
pub fn bootstrap_phase_from_u8(v: u8) -> Result<PluginBootstrapPhase, String> {
    match v {
        1 => Ok(PluginBootstrapPhase::Bootstrap),
        2 => Ok(PluginBootstrapPhase::Platform),
        3 => Ok(PluginBootstrapPhase::Engine),
        _ => Err(format!("invalid bootstrap phase {v}")),
    }
}
fn role_to_u8(v: CapabilityRole) -> u8 {
    v as u8
}
fn role_from_u8(v: u8) -> Result<CapabilityRole, String> {
    match v {
        1 => Ok(CapabilityRole::Provides),
        2 => Ok(CapabilityRole::Requires),
        _ => Err(format!("invalid capability role {v}")),
    }
}
fn cap_kind_to_u8(v: CapabilityKind) -> u8 {
    v as u8
}
fn cap_kind_from_u8(v: u8) -> Result<CapabilityKind, String> {
    match v {
        1 => Ok(CapabilityKind::ServiceV1),
        2 => Ok(CapabilityKind::EventsV1),
        3 => Ok(CapabilityKind::AssetImporterV1),
        4 => Ok(CapabilityKind::SceneContributionV1),
        255 => Ok(CapabilityKind::Other),
        _ => Err(format!("invalid capability kind {v}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_descriptor_json_roundtrip_preserves_composition_semantics() {
        let capability = CapabilityDescV2 {
            id: "render.backend".into(),
            role: CapabilityRole::Provides,
            kind: CapabilityKind::Other,
            version: 7,
            contract: ROption::RSome(ContractRefV2::new("newengine.render-provider", 3)),
            tags: vec![SystemTagV2::new("headful"), SystemTagV2::new("gpu")].into(),
            route: ROption::RSome(BackendRouteDescriptorV2 {
                service_kind: "render".into(),
                engine_gateway: "engine.render".into(),
                provider_service_id: "render.api".into(),
                provider_abi: ROption::RSome("newengine.render-provider/v3".into()),
                provider_route: ROption::RSome("engine.render.test".into()),
                backend_priority: 77,
                backend: ROption::RSome("test-gpu".into()),
                mode: ROption::RSome("graph".into()),
                features: vec![RString::from("timeline"), RString::from("bindless")].into(),
            }),
            requirement: ROption::RNone,
            extension_json: "domain-only".into(),
        };
        let descriptor = PluginDescriptorV2 {
            id: "engine.render.test".into(),
            name: "Test Renderer".into(),
            version: "4.5.6".into(),
            kind: PluginKind::Runtime,
            capabilities: vec![capability].into(),
            extension_json: r#"{"runtime_units":[{"id":"test.unit","version":1,"kind":"product_extension","provides":[],"requires":[],"tags":[]}]}"#.into(),
        };

        let projection = PluginDiscoveryDescriptorV1::from_descriptor_v2(&descriptor);
        let json = serde_json::to_string(&projection).expect("serialize");
        let decoded: PluginDiscoveryDescriptorV1 =
            serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded, projection);

        let restored = decoded
            .to_descriptor_v2()
            .expect("restore typed descriptor");
        assert_eq!(
            PluginDiscoveryDescriptorV1::from_descriptor_v2(&restored),
            projection
        );
    }
}
