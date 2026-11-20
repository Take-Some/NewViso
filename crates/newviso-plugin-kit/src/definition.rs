#![forbid(unsafe_op_in_unsafe_fn)]

//! Declarative plugin-definition helpers.
//!
//! This is the North Star equivalent of the old two-file plugin templates: plugin
//! authors describe identity, services and backend routes in data, while the kit
//! owns descriptor construction and ABI boilerplate.

use crate::plugin_api::{
    BackendRouteDescriptor, BackendServiceSpec, CapabilityDesc, CapabilityDescV2, CapabilityKind,
    CapabilityRole, ContractCompatibility, ContractKind, ContractVersion, PluginDescriptor,
    PluginDescriptorV2, PluginKind, RuntimeContractDeclaration,
};
use abi_stable::std_types::RString;
use serde_json::Value;

#[derive(Debug, Clone, Copy)]
pub struct PluginDefinition {
    pub id: &'static str,
    pub name: &'static str,
    pub version: &'static str,
    pub kind: PluginKind,
    pub services: &'static [PluginServiceDefinition],
    pub backend_routes: &'static [PluginBackendRouteDefinition],
    pub capabilities: &'static [PluginCapabilityDefinition],
}

impl PluginDefinition {
    #[inline]
    pub fn descriptor(self) -> PluginDescriptor {
        descriptor_from_definition(self)
    }

    #[inline]
    pub fn descriptor_v2(self) -> PluginDescriptorV2 {
        descriptor_v2_from_definition(self)
    }

    /// Adds runtime extension contracts without changing the stable PluginDefinition
    /// layout used by existing plugin source. The returned wrapper exposes the same
    /// descriptor()/descriptor_v2() authoring surface.
    #[inline]
    pub const fn with_contracts(
        self,
        contracts: &'static [PluginContractDefinition],
    ) -> PluginDefinitionWithContracts {
        PluginDefinitionWithContracts {
            definition: self,
            contracts,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PluginDefinitionWithContracts {
    pub definition: PluginDefinition,
    pub contracts: &'static [PluginContractDefinition],
}

impl PluginDefinitionWithContracts {
    #[inline]
    pub fn descriptor(self) -> PluginDescriptor {
        descriptor_with_contracts(self.definition, self.contracts)
    }

    #[inline]
    pub fn descriptor_v2(self) -> PluginDescriptorV2 {
        let mut descriptor = descriptor_v2_from_definition(self.definition);
        for contract in self.contracts {
            descriptor
                .capabilities
                .push(contract_capability_v2(*contract));
        }
        descriptor
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PluginContractDefinition {
    pub key: &'static str,
    pub kind: ContractKind,
    pub version: ContractVersion,
    pub compatibility: ContractCompatibility,
    pub advertised_id: Option<&'static str>,
}

#[inline]
pub const fn plugin_contract(
    key: &'static str,
    kind: ContractKind,
    version: ContractVersion,
    compatibility: ContractCompatibility,
    advertised_id: Option<&'static str>,
) -> PluginContractDefinition {
    PluginContractDefinition {
        key,
        kind,
        version,
        compatibility,
        advertised_id,
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PluginServiceDefinition {
    pub id: &'static str,
    pub version: u32,
    pub describe_json: &'static str,
}

#[derive(Debug, Clone, Copy)]
pub struct PluginBackendRouteDefinition {
    pub capability_id: &'static str,
    pub spec: BackendServiceSpec,
    pub provider_abi: Option<&'static str>,
    pub provider_route: Option<&'static str>,
    pub backend: Option<&'static str>,
    pub mode: Option<&'static str>,
    pub priority: i32,
    pub features: &'static [&'static str],
    pub system_tags: &'static [&'static str],
    pub metadata_json: &'static [PluginMetadataJson],
}

#[derive(Debug, Clone, Copy)]
pub struct PluginMetadataJson {
    pub key: &'static str,
    pub value_json: &'static str,
}

#[derive(Debug, Clone, Copy)]
pub struct PluginCapabilityDefinition {
    pub id: &'static str,
    pub role: CapabilityRole,
    pub kind: CapabilityKind,
    pub version: u32,
    pub describe_json: &'static str,
}

#[inline]
pub const fn plugin_service(
    id: &'static str,
    version: u32,
    describe_json: &'static str,
) -> PluginServiceDefinition {
    PluginServiceDefinition {
        id,
        version,
        describe_json,
    }
}

#[inline]
pub const fn metadata_json(key: &'static str, value_json: &'static str) -> PluginMetadataJson {
    PluginMetadataJson { key, value_json }
}

#[inline]
pub const fn provided_capability(
    id: &'static str,
    kind: CapabilityKind,
    version: u32,
    describe_json: &'static str,
) -> PluginCapabilityDefinition {
    PluginCapabilityDefinition {
        id,
        role: CapabilityRole::Provides,
        kind,
        version,
        describe_json,
    }
}

#[inline]
#[allow(clippy::too_many_arguments)]
pub const fn optional_backend_route(
    capability_id: &'static str,
    spec: BackendServiceSpec,
    provider_route: Option<&'static str>,
    backend: Option<&'static str>,
    mode: Option<&'static str>,
    priority: i32,
    features: &'static [&'static str],
    system_tags: &'static [&'static str],
    metadata_json: &'static [PluginMetadataJson],
) -> PluginBackendRouteDefinition {
    PluginBackendRouteDefinition {
        capability_id,
        spec,
        provider_abi: None,
        provider_route,
        backend,
        mode,
        priority,
        features,
        system_tags,
        metadata_json,
    }
}

#[inline]
#[allow(clippy::too_many_arguments)]
pub const fn optional_backend_route_with_abi(
    capability_id: &'static str,
    spec: BackendServiceSpec,
    provider_abi: &'static str,
    provider_route: Option<&'static str>,
    backend: Option<&'static str>,
    mode: Option<&'static str>,
    priority: i32,
    features: &'static [&'static str],
    system_tags: &'static [&'static str],
    metadata_json: &'static [PluginMetadataJson],
) -> PluginBackendRouteDefinition {
    PluginBackendRouteDefinition {
        capability_id,
        spec,
        provider_abi: Some(provider_abi),
        provider_route,
        backend,
        mode,
        priority,
        features,
        system_tags,
        metadata_json,
    }
}

#[inline]
pub fn descriptor_from_definition(def: PluginDefinition) -> PluginDescriptor {
    let mut builder = PluginDescriptor::builder(def.id, def.name, def.version, def.kind);

    for service in def.services {
        builder = builder.provides_service(
            service.id,
            service.version,
            RString::from(service.describe_json),
        );
    }

    for route in def.backend_routes {
        let mut desc = BackendRouteDescriptor::new(route.spec).priority(route.priority);
        if let Some(provider_abi) = route.provider_abi {
            desc = desc.provider_abi(provider_abi);
        }
        if let Some(provider_route) = route.provider_route {
            desc = desc.provider_route(provider_route);
        }
        if let Some(backend) = route.backend {
            desc = desc.backend(backend);
        }
        if let Some(mode) = route.mode {
            desc = desc.mode(mode);
        }
        if !route.features.is_empty() {
            desc = desc.features(route.features.iter().copied());
        }
        if !route.system_tags.is_empty() {
            desc = desc.system_tags(route.system_tags.iter().copied());
        }
        for item in route.metadata_json {
            let value = parse_metadata_value(item.key, item.value_json);
            desc = desc.metadata_json(item.key, value);
        }
        builder = builder.push(CapabilityDesc::backend_route(route.capability_id, desc));
    }

    for cap in def.capabilities {
        builder = builder.push(
            CapabilityDesc::new(cap.id, cap.role, cap.kind, cap.version)
                .with_json(RString::from(cap.describe_json)),
        );
    }

    builder.build()
}

pub fn descriptor_with_contracts(
    def: PluginDefinition,
    contracts: &[PluginContractDefinition],
) -> PluginDescriptor {
    let mut descriptor = descriptor_from_definition(def);
    for contract in contracts {
        descriptor.capabilities.push(contract_capability(*contract));
    }
    descriptor
}

fn contract_capability(contract: PluginContractDefinition) -> CapabilityDesc {
    let mut declaration = RuntimeContractDeclaration::new(
        contract.key,
        contract.kind,
        contract.version,
        contract.compatibility,
    );
    if let Some(advertised_id) = contract.advertised_id {
        declaration = declaration.advertised_id(advertised_id);
    }
    declaration.into_capability()
}

pub fn descriptor_v2_from_definition(def: PluginDefinition) -> PluginDescriptorV2 {
    let mut builder = PluginDescriptorV2::builder(def.id, def.name, def.version, def.kind);

    for service in def.services {
        builder = builder.provides_service(
            service.id,
            service.version,
            RString::from(service.describe_json),
        );
    }

    for route in def.backend_routes {
        let mut descriptor = BackendRouteDescriptor::new(route.spec).priority(route.priority);
        if let Some(provider_abi) = route.provider_abi {
            descriptor = descriptor.provider_abi(provider_abi);
        }
        if let Some(provider_route) = route.provider_route {
            descriptor = descriptor.provider_route(provider_route);
        }
        if let Some(backend) = route.backend {
            descriptor = descriptor.backend(backend);
        }
        if let Some(mode) = route.mode {
            descriptor = descriptor.mode(mode);
        }
        if !route.features.is_empty() {
            descriptor = descriptor.features(route.features.iter().copied());
        }
        if !route.system_tags.is_empty() {
            descriptor = descriptor.system_tags(route.system_tags.iter().copied());
        }
        for item in route.metadata_json {
            descriptor =
                descriptor.metadata_json(item.key, parse_metadata_value(item.key, item.value_json));
        }
        builder = builder.push(CapabilityDescV2::backend_route(
            route.capability_id,
            1,
            descriptor,
        ));
    }

    for capability in def.capabilities {
        builder = builder.push(
            CapabilityDescV2::new(
                capability.id,
                capability.role,
                capability.kind,
                capability.version,
            )
            .with_extension_json(RString::from(capability.describe_json)),
        );
    }

    builder.build()
}

fn contract_capability_v2(contract: PluginContractDefinition) -> CapabilityDescV2 {
    let declaration = RuntimeContractDeclaration::new(
        contract.key,
        contract.kind,
        contract.version,
        contract.compatibility,
    );
    let declaration = if let Some(advertised_id) = contract.advertised_id {
        declaration.advertised_id(advertised_id)
    } else {
        declaration
    };
    let key = declaration.key.trim();
    let metadata = serde_json::json!({
        "runtime_contract": {
            "kind": declaration.kind.as_str(),
            "version": {
                "major": declaration.version.major,
                "minor": declaration.version.minor,
                "patch": declaration.version.patch,
            },
            "compatibility": match declaration.compatibility {
                ContractCompatibility::Exact => "exact",
                ContractCompatibility::SameMajor => "same_major",
                ContractCompatibility::AtLeast => "at_least",
            },
            "advertised_id": declaration.advertised_id,
        }
    });
    CapabilityDescV2::new(
        format!(
            "{}{}",
            crate::plugin_api::RUNTIME_CONTRACT_CAPABILITY_PREFIX,
            key
        ),
        CapabilityRole::Provides,
        CapabilityKind::Other,
        1,
    )
    .with_extension_json(metadata.to_string())
}

fn parse_metadata_value(key: &str, raw: &str) -> Value {
    serde_json::from_str(raw)
        .unwrap_or_else(|e| panic!("invalid plugin metadata JSON for key '{key}': {e}: {raw}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SERVICES: &[PluginServiceDefinition] = &[];
    const ROUTES: &[PluginBackendRouteDefinition] = &[];
    const CAPABILITIES: &[PluginCapabilityDefinition] = &[];
    const CONTRACTS: &[PluginContractDefinition] = &[plugin_contract(
        "acme.extension.protocol",
        ContractKind::Protocol,
        ContractVersion::major(1),
        ContractCompatibility::SameMajor,
        Some("acme.extension/v1"),
    )];
    const DEFINITION: PluginDefinition = PluginDefinition {
        id: "acme.plugin",
        name: "Acme",
        version: "1.0.0",
        kind: PluginKind::Runtime,
        services: SERVICES,
        backend_routes: ROUTES,
        capabilities: CAPABILITIES,
    };

    #[test]
    fn definition_with_contracts_preserves_legacy_definition_shape() {
        let descriptor = DEFINITION.with_contracts(CONTRACTS).descriptor();
        let contracts = descriptor
            .capabilities
            .iter()
            .filter_map(|capability| {
                crate::plugin_api::runtime_contract_declaration(capability).transpose()
            })
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(contracts.len(), 1);
        assert_eq!(contracts[0].key, "acme.extension.protocol");
        assert_eq!(
            contracts[0].advertised_id.as_deref(),
            Some("acme.extension/v1")
        );
    }
}
