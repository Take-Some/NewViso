#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, StableAbi)]
pub enum PluginKind {
    Runtime = 1,
    Importer = 2,
    Editor = 3,
    Tool = 4,
    Other = 255,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, StableAbi)]
pub enum CapabilityRole {
    Provides = 1,
    Requires = 2,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, StableAbi)]
pub enum CapabilityKind {
    ServiceV1 = 1,
    EventsV1 = 2,
    AssetImporterV1 = 3,
    SceneContributionV1 = 4,
    Other = 255,
}

/// Stable ABI contract reference used by typed capability metadata.
/// `version = RNone` means that a legacy descriptor named the contract but did
/// not provide an ABI contract version. New V2 producers should set it.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq, StableAbi)]
pub struct ContractRefV2 {
    pub id: RString,
    pub version: ROption<u32>,
}

impl ContractRefV2 {
    #[inline]
    pub fn new(id: impl Into<RString>, version: u32) -> Self {
        Self {
            id: id.into(),
            version: ROption::RSome(version),
        }
    }

    #[inline]
    pub fn unversioned(id: impl Into<RString>) -> Self {
        Self {
            id: id.into(),
            version: ROption::RNone,
        }
    }
}

/// Stable ABI system/composition tag. Unlike `newengine_service_api::SystemTag`,
/// this representation owns its string and is safe across a dynamic-library ABI.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq, StableAbi)]
pub struct SystemTagV2(pub RString);

impl SystemTagV2 {
    #[inline]
    pub fn new(value: impl Into<RString>) -> Self {
        Self(value.into())
    }

    #[inline]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

/// Typed provider-route metadata consumed by composition/routing.
/// JSON is deliberately excluded from route-critical fields.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq, StableAbi)]
pub struct BackendRouteDescriptorV2 {
    pub service_kind: RString,
    pub engine_gateway: RString,
    pub provider_service_id: RString,
    pub provider_abi: ROption<RString>,
    pub provider_route: ROption<RString>,
    pub backend_priority: i32,
    pub backend: ROption<RString>,
    pub mode: ROption<RString>,
    pub features: RVec<RString>,
}

impl BackendRouteDescriptorV2 {
    #[inline]
    pub fn new(
        service_kind: impl Into<RString>,
        engine_gateway: impl Into<RString>,
        provider_service_id: impl Into<RString>,
    ) -> Self {
        Self {
            service_kind: service_kind.into(),
            engine_gateway: engine_gateway.into(),
            provider_service_id: provider_service_id.into(),
            provider_abi: ROption::RNone,
            provider_route: ROption::RNone,
            backend_priority: 0,
            backend: ROption::RNone,
            mode: ROption::RNone,
            features: RVec::new(),
        }
    }
}

/// Typed requirement semantics for `CapabilityRole::Requires`.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq, StableAbi)]
pub struct CapabilityRequirementDescV2 {
    pub min_version: u32,
    pub max_version: ROption<u32>,
    pub required_tags: RVec<SystemTagV2>,
    pub preferred_tags: RVec<SystemTagV2>,
    pub forbidden_tags: RVec<SystemTagV2>,
}

impl CapabilityRequirementDescV2 {
    #[inline]
    pub fn at_least(min_version: u32) -> Self {
        Self {
            min_version,
            max_version: ROption::RNone,
            required_tags: RVec::new(),
            preferred_tags: RVec::new(),
            forbidden_tags: RVec::new(),
        }
    }

    #[inline]
    pub fn between(min_version: u32, max_version: u32) -> Self {
        let mut out = Self::at_least(min_version);
        out.max_version = ROption::RSome(max_version);
        out
    }

    #[inline]
    pub fn with_required_tag(mut self, tag: impl Into<RString>) -> Self {
        push_unique_tag(&mut self.required_tags, tag.into().as_str());
        self
    }

    #[inline]
    pub fn with_preferred_tag(mut self, tag: impl Into<RString>) -> Self {
        push_unique_tag(&mut self.preferred_tags, tag.into().as_str());
        self
    }

    #[inline]
    pub fn with_forbidden_tag(mut self, tag: impl Into<RString>) -> Self {
        push_unique_tag(&mut self.forbidden_tags, tag.into().as_str());
        self
    }
}

/// V2 capability metadata: all semantics used by composition, routing and
/// validation are typed. `extension_json` is reserved for domain-specific data
/// that does not participate in generic host decisions.
#[repr(C)]
#[derive(Debug, Clone, StableAbi)]
pub struct CapabilityDescV2 {
    pub id: CapabilityId,
    pub role: CapabilityRole,
    pub kind: CapabilityKind,
    pub version: u32,
    pub contract: ROption<ContractRefV2>,
    pub tags: RVec<SystemTagV2>,
    pub route: ROption<BackendRouteDescriptorV2>,
    pub requirement: ROption<CapabilityRequirementDescV2>,
    pub extension_json: RString,
}

impl CapabilityDescV2 {
    #[inline]
    pub fn new(
        id: impl Into<CapabilityId>,
        role: CapabilityRole,
        kind: CapabilityKind,
        version: u32,
    ) -> Self {
        Self {
            id: id.into(),
            role,
            kind,
            version,
            contract: ROption::RNone,
            tags: RVec::new(),
            route: ROption::RNone,
            requirement: if role == CapabilityRole::Requires {
                ROption::RSome(CapabilityRequirementDescV2::at_least(version))
            } else {
                ROption::RNone
            },
            extension_json: RString::new(),
        }
    }

    #[inline]
    pub fn with_contract(mut self, contract: ContractRefV2) -> Self {
        self.contract = ROption::RSome(contract);
        self
    }

    #[inline]
    pub fn with_requirement(mut self, requirement: CapabilityRequirementDescV2) -> Self {
        self.requirement = ROption::RSome(requirement);
        self
    }

    #[inline]
    pub fn with_tag(mut self, tag: impl Into<RString>) -> Self {
        let tag = SystemTagV2::new(tag);
        if !self
            .tags
            .iter()
            .any(|existing| existing.as_str() == tag.as_str())
        {
            self.tags.push(tag);
        }
        self
    }

    #[inline]
    pub fn with_route(mut self, route: BackendRouteDescriptorV2) -> Self {
        self.route = ROption::RSome(route);
        self
    }

    #[inline]
    pub fn with_extension_json(mut self, json: impl Into<RString>) -> Self {
        self.extension_json = json.into();
        self
    }

    #[inline]
    pub fn has_tag(&self, tag: &str) -> bool {
        let tag = tag.trim();
        !tag.is_empty() && self.tags.iter().any(|candidate| candidate.as_str() == tag)
    }

    /// Builds typed V2 route metadata without serializing routing semantics to JSON.
    pub fn backend_route(
        id: impl Into<CapabilityId>,
        version: u32,
        descriptor: BackendRouteDescriptor,
    ) -> Self {
        let mut tags = RVec::new();
        for tag in descriptor.system_tags.iter().copied() {
            push_unique_tag(&mut tags, tag);
        }
        if let Some(backend) = descriptor.backend {
            push_unique_tag(
                &mut tags,
                &format!("backend.{}", metadata_tag_slug(backend)),
            );
        }
        if let Some(mode) = descriptor.mode {
            push_unique_tag(&mut tags, &format!("mode.{}", metadata_tag_slug(mode)));
        }
        for feature in descriptor.features.iter().copied() {
            push_unique_tag(
                &mut tags,
                &format!("feature.{}", metadata_tag_slug(feature)),
            );
        }
        let extension_json = if descriptor.metadata.is_empty() {
            RString::new()
        } else {
            RString::from(serde_json::to_string(&descriptor.metadata).unwrap_or_default())
        };
        let route = BackendRouteDescriptorV2 {
            service_kind: RString::from(descriptor.service_kind),
            engine_gateway: RString::from(descriptor.engine_gateway),
            provider_service_id: RString::from(descriptor.contract),
            provider_abi: descriptor.provider_abi.map(RString::from).into(),
            provider_route: descriptor.provider_route.map(RString::from).into(),
            backend_priority: descriptor.backend_priority,
            backend: descriptor.backend.map(RString::from).into(),
            mode: descriptor.mode.map(RString::from).into(),
            features: descriptor
                .features
                .into_iter()
                .map(RString::from)
                .collect::<Vec<_>>()
                .into(),
        };
        Self {
            id: id.into(),
            role: CapabilityRole::Provides,
            kind: CapabilityKind::Other,
            version,
            contract: ROption::RSome(ContractRefV2::unversioned(
                route.provider_service_id.clone(),
            )),
            tags,
            route: ROption::RSome(route),
            requirement: ROption::RNone,
            extension_json,
        }
    }

    /// Canonical V1 -> V2 compatibility normalization. Generic host code should
    /// call this instead of parsing `describe_json` itself.
    pub fn from_legacy(capability: &CapabilityDesc) -> Self {
        let mut out = Self::new(
            capability.id.clone(),
            capability.role,
            capability.kind,
            capability.version,
        );
        let raw = capability.describe_json.as_str().trim();
        if raw.is_empty() {
            return out;
        }
        out.extension_json = capability.describe_json.clone();
        let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) else {
            return out;
        };

        if let Some(contract) = json_string(&value, "contract") {
            out.contract = ROption::RSome(ContractRefV2::unversioned(contract));
        }

        collect_json_tags(&value, "tags", &mut out.tags);
        collect_json_tags(&value, "system_tags", &mut out.tags);
        if let Some(backend) = json_string(&value, "backend") {
            push_unique_tag(
                &mut out.tags,
                &format!("backend.{}", metadata_tag_slug(&backend)),
            );
        }
        if let Some(mode) = json_string(&value, "mode") {
            push_unique_tag(&mut out.tags, &format!("mode.{}", metadata_tag_slug(&mode)));
        }
        match value.get("features") {
            Some(serde_json::Value::Array(values)) => {
                for feature in values.iter().filter_map(serde_json::Value::as_str) {
                    push_unique_tag(
                        &mut out.tags,
                        &format!("feature.{}", metadata_tag_slug(feature)),
                    );
                }
            }
            Some(serde_json::Value::String(feature)) => {
                push_unique_tag(
                    &mut out.tags,
                    &format!("feature.{}", metadata_tag_slug(feature)),
                );
            }
            _ => {}
        }

        let gateway = json_string(&value, "engine_gateway");
        let service_kind = json_string(&value, "service_kind");
        if let (Some(engine_gateway), Some(service_kind)) = (gateway, service_kind) {
            let provider_service_id = json_string(&value, "contract").unwrap_or_default();
            out.route = ROption::RSome(BackendRouteDescriptorV2 {
                service_kind: service_kind.into(),
                engine_gateway: engine_gateway.into(),
                provider_service_id: provider_service_id.into(),
                provider_abi: json_string(&value, "provider_abi")
                    .map(RString::from)
                    .into(),
                provider_route: json_string(&value, "provider_route")
                    .map(RString::from)
                    .into(),
                backend_priority: value
                    .get("backend_priority")
                    .and_then(serde_json::Value::as_i64)
                    .unwrap_or(0)
                    .clamp(i32::MIN as i64, i32::MAX as i64)
                    as i32,
                backend: json_string(&value, "backend").map(RString::from).into(),
                mode: json_string(&value, "mode").map(RString::from).into(),
                features: json_strings(&value, "features")
                    .into_iter()
                    .map(RString::from)
                    .collect::<Vec<_>>()
                    .into(),
            });
        }
        out
    }
}
