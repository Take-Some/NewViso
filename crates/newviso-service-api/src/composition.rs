/// Stable logical capability identity used by composition requirements.
///
/// The public identity (`id`) is what products such as GameReady ask for. The
/// gateway/service-kind binding is shared contract vocabulary and is intentionally
/// hidden from product profiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CapabilityId {
    id: &'static str,
    gateway_id: &'static str,
    service_kind: &'static str,
}

impl CapabilityId {
    #[inline]
    pub const fn new(
        id: &'static str,
        gateway_id: &'static str,
        service_kind: &'static str,
    ) -> Self {
        Self {
            id,
            gateway_id,
            service_kind,
        }
    }

    #[inline]
    pub const fn as_str(self) -> &'static str {
        self.id
    }

    #[inline]
    pub const fn gateway_id(self) -> &'static str {
        self.gateway_id
    }

    #[inline]
    pub const fn service_kind(self) -> &'static str {
        self.service_kind
    }
}

/// Declarative system tag used by the composition solver.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SystemTag(&'static str);

impl SystemTag {
    #[inline]
    pub const fn new(value: &'static str) -> Self {
        Self(value)
    }

    #[inline]
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

/// Requirement strength for one composition capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RequirementStrength {
    Optional,
    Preferred,
    Required,
}

impl RequirementStrength {
    #[inline]
    pub const fn is_required(self) -> bool {
        matches!(self, Self::Required)
    }
}

/// Solver cardinality policy.
///
/// `Many` composes with requirement strength: a required-many capability resolves
/// at least one provider, while optional/preferred-many may resolve zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cardinality {
    One,
    ZeroOrOne,
    Many,
}

impl Default for Cardinality {
    #[inline]
    fn default() -> Self {
        Self::One
    }
}

impl Cardinality {
    #[inline]
    pub const fn min(self, strength: RequirementStrength) -> u16 {
        match self {
            Self::One => 1,
            Self::ZeroOrOne => 0,
            Self::Many if matches!(strength, RequirementStrength::Required) => 1,
            Self::Many => 0,
        }
    }

    #[inline]
    pub const fn max(self) -> u16 {
        match self {
            Self::One | Self::ZeroOrOne => 1,
            Self::Many => u16::MAX,
        }
    }
}

/// Versioned provider contract requirement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContractRequirement {
    pub contract_id: &'static str,
    pub min_version: u32,
    pub max_version: Option<u32>,
}

impl ContractRequirement {
    #[inline]
    pub const fn new(
        contract_id: &'static str,
        min_version: u32,
        max_version: Option<u32>,
    ) -> Self {
        Self {
            contract_id,
            min_version,
            max_version,
        }
    }
}

/// Explicit fallback policy for an unsatisfied primary provider set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FallbackPolicy {
    None,
    Providers(&'static [&'static str]),
}

impl FallbackPolicy {
    #[inline]
    pub const fn providers(self) -> &'static [&'static str] {
        match self {
            Self::None => &[],
            Self::Providers(providers) => providers,
        }
    }
}

/// V2 composition requirement consumed directly by the shared solver.
///
/// Product profiles describe *what* they need, not which runtime modules should
/// be registered to satisfy it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapabilityRequirement {
    pub capability: CapabilityId,
    pub contract: Option<ContractRequirement>,
    pub strength: RequirementStrength,
    pub cardinality: Cardinality,
    /// Hard feature/tag requirements. Retained as an advanced constraint in
    /// addition to the simpler preferred/forbidden policy surface.
    pub required_tags: &'static [SystemTag],
    pub preferred_tags: &'static [SystemTag],
    pub forbidden_tags: &'static [SystemTag],
    pub fallback: FallbackPolicy,
}

impl CapabilityRequirement {
    #[inline]
    pub const fn new(capability: CapabilityId, strength: RequirementStrength) -> Self {
        Self {
            capability,
            contract: None,
            strength,
            cardinality: match strength {
                RequirementStrength::Required => Cardinality::One,
                RequirementStrength::Optional | RequirementStrength::Preferred => {
                    Cardinality::ZeroOrOne
                }
            },
            required_tags: &[],
            preferred_tags: &[],
            forbidden_tags: &[],
            fallback: FallbackPolicy::None,
        }
    }

    #[inline]
    pub const fn required(capability: CapabilityId) -> Self {
        Self::new(capability, RequirementStrength::Required)
    }

    #[inline]
    pub const fn optional(capability: CapabilityId) -> Self {
        Self::new(capability, RequirementStrength::Optional)
    }

    #[inline]
    pub const fn preferred(capability: CapabilityId) -> Self {
        Self::new(capability, RequirementStrength::Preferred)
    }

    #[inline]
    pub const fn with_contract(
        mut self,
        contract_id: &'static str,
        min_version: u32,
        max_version: Option<u32>,
    ) -> Self {
        self.contract = Some(ContractRequirement::new(
            contract_id,
            min_version,
            max_version,
        ));
        self
    }

    #[inline]
    pub const fn with_cardinality(mut self, cardinality: Cardinality) -> Self {
        self.cardinality = cardinality;
        self
    }

    #[inline]
    pub const fn with_required_tags(mut self, tags: &'static [SystemTag]) -> Self {
        self.required_tags = tags;
        self
    }

    #[inline]
    pub const fn with_preferred_tags(mut self, tags: &'static [SystemTag]) -> Self {
        self.preferred_tags = tags;
        self
    }

    #[inline]
    pub const fn with_forbidden_tags(mut self, tags: &'static [SystemTag]) -> Self {
        self.forbidden_tags = tags;
        self
    }

    #[inline]
    pub const fn with_fallback(mut self, fallback: FallbackPolicy) -> Self {
        self.fallback = fallback;
        self
    }
}

/// V1 compatibility descriptor. New composition code should use
/// `CapabilityRequirement`; this type remains only for gradual migration of old
/// profile declarations and diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EngineCapabilitySlotSpec {
    pub gateway_id: &'static str,
    pub service_kind: &'static str,
    pub required: bool,
}

impl EngineCapabilitySlotSpec {
    #[inline]
    pub const fn new(gateway_id: &'static str, service_kind: &'static str, required: bool) -> Self {
        Self {
            gateway_id,
            service_kind,
            required,
        }
    }

    #[inline]
    pub const fn required(gateway_id: &'static str, service_kind: &'static str) -> Self {
        Self::new(gateway_id, service_kind, true)
    }

    #[inline]
    pub const fn optional(gateway_id: &'static str, service_kind: &'static str) -> Self {
        Self::new(gateway_id, service_kind, false)
    }

    #[inline]
    pub const fn into_v2(self) -> CapabilityRequirement {
        let capability = CapabilityId::new(self.gateway_id, self.gateway_id, self.service_kind);
        if self.required {
            CapabilityRequirement::required(capability)
        } else {
            CapabilityRequirement::optional(capability)
        }
    }
}

// Transitional aliases for callers introduced during the first V2 migration pass.
pub type EngineCapabilityRequirementSpec = CapabilityRequirement;
pub type CapabilityRequirementLevel = RequirementStrength;
pub type CapabilityContractRequirement = ContractRequirement;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineRuntimeUnitKind {
    Module,
    Adapter,
    Provider,
    ProductExtension,
}

/// Static runtime unit descriptor. The descriptor is pure data; the generic host
/// owns the instance-local factory catalog that materializes matching unit ids.
/// `provides` is capability/service vocabulary contributed by the unit; `requires`
/// is dependency vocabulary that must be supplied by the composition or another unit.
/// Product profiles select stable runtime-unit ids, never concrete Rust module types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EngineRuntimeUnitSpec {
    pub id: &'static str,
    pub version: u32,
    pub kind: EngineRuntimeUnitKind,
    pub provides: &'static [&'static str],
    pub requires: &'static [&'static str],
    pub tags: &'static [&'static str],
}

impl EngineRuntimeUnitSpec {
    #[inline]
    pub const fn new(
        id: &'static str,
        version: u32,
        kind: EngineRuntimeUnitKind,
        provides: &'static [&'static str],
        requires: &'static [&'static str],
        tags: &'static [&'static str],
    ) -> Self {
        Self {
            id,
            version,
            kind,
            provides,
            requires,
            tags,
        }
    }
}

/// Owned runtime-unit descriptor used when inventory comes from dynamic sources such as
/// game manifests or plugin descriptor metadata. Static engine/profile declarations convert
/// losslessly into this representation before inventory merge and solving.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RuntimeUnitDescriptor {
    pub id: String,
    pub version: u32,
    pub kind: EngineRuntimeUnitKind,
    #[serde(default)]
    pub provides: Vec<String>,
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

impl RuntimeUnitDescriptor {
    #[inline]
    pub fn from_static(spec: EngineRuntimeUnitSpec) -> Self {
        Self {
            id: spec.id.to_owned(),
            version: spec.version,
            kind: spec.kind,
            provides: spec
                .provides
                .iter()
                .map(|value| (*value).to_owned())
                .collect(),
            requires: spec
                .requires
                .iter()
                .map(|value| (*value).to_owned())
                .collect(),
            tags: spec.tags.iter().map(|value| (*value).to_owned()).collect(),
        }
    }

    #[inline]
    pub fn candidate_key(&self) -> String {
        format!("{}@{}", self.id, self.version)
    }
}

impl From<EngineRuntimeUnitSpec> for RuntimeUnitDescriptor {
    #[inline]
    fn from(spec: EngineRuntimeUnitSpec) -> Self {
        Self::from_static(spec)
    }
}

/// Canonical runtime-unit capability ids shared by declarative composition producers and consumers.
/// These ids describe runtime behavior, not provider implementations, and deliberately live below
/// product/game-module APIs so no domain needs to depend on a legacy role enum.
pub mod runtime_unit_capability {
    pub const GAME_SCENE_BOOTSTRAP: &str = "game.scene.bootstrap";
    pub const GAME_WORLD_RUNTIME: &str = "game.world.runtime";
    pub const GAME_INPUT_PROFILE: &str = "game.input.profile";
    pub const RENDER_FEATURE: &str = "render.feature";
}

/// Runtime-unit-only capability requirement consumed by the shared composition solver.
/// Unlike `CapabilityRequirement`, this does not imply an engine gateway/service route.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeUnitRequirementSpec {
    pub capability: &'static str,
    pub strength: RequirementStrength,
    pub cardinality: Cardinality,
    pub required_tags: &'static [SystemTag],
    pub preferred_tags: &'static [SystemTag],
    pub forbidden_tags: &'static [SystemTag],
}

impl RuntimeUnitRequirementSpec {
    #[inline]
    pub const fn new(capability: &'static str, strength: RequirementStrength) -> Self {
        Self {
            capability,
            strength,
            cardinality: match strength {
                RequirementStrength::Required => Cardinality::One,
                RequirementStrength::Optional | RequirementStrength::Preferred => {
                    Cardinality::ZeroOrOne
                }
            },
            required_tags: &[],
            preferred_tags: &[],
            forbidden_tags: &[],
        }
    }

    #[inline]
    pub const fn required(capability: &'static str) -> Self {
        Self::new(capability, RequirementStrength::Required)
    }

    #[inline]
    pub const fn optional(capability: &'static str) -> Self {
        Self::new(capability, RequirementStrength::Optional)
    }

    #[inline]
    pub const fn preferred(capability: &'static str) -> Self {
        Self::new(capability, RequirementStrength::Preferred)
    }

    #[inline]
    pub const fn with_cardinality(mut self, cardinality: Cardinality) -> Self {
        self.cardinality = cardinality;
        self
    }

    #[inline]
    pub const fn with_required_tags(mut self, tags: &'static [SystemTag]) -> Self {
        self.required_tags = tags;
        self
    }

    #[inline]
    pub const fn with_preferred_tags(mut self, tags: &'static [SystemTag]) -> Self {
        self.preferred_tags = tags;
        self
    }

    #[inline]
    pub const fn with_forbidden_tags(mut self, tags: &'static [SystemTag]) -> Self {
        self.forbidden_tags = tags;
        self
    }
}

/// Owned runtime/wire representation of a runtime-unit capability requirement.
///
/// Static engine/profile declarations use [`RuntimeUnitRequirementSpec`]; dynamic sources such as
/// GameModule descriptors use this type so capability/tag vocabulary remains owned and does not
/// require leaking strings into process lifetime.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct RuntimeUnitRequirementDescriptor {
    pub capability: String,
    pub required: bool,
    pub cardinality: Cardinality,
    pub required_tags: Vec<String>,
    pub preferred_tags: Vec<String>,
    pub forbidden_tags: Vec<String>,
}

impl Default for RuntimeUnitRequirementDescriptor {
    fn default() -> Self {
        Self {
            capability: String::new(),
            required: true,
            cardinality: Cardinality::One,
            required_tags: Vec::new(),
            preferred_tags: Vec::new(),
            forbidden_tags: Vec::new(),
        }
    }
}

impl RuntimeUnitRequirementDescriptor {
    #[inline]
    pub fn required(capability: impl Into<String>) -> Self {
        Self {
            capability: capability.into(),
            ..Self::default()
        }
    }

    #[inline]
    pub fn optional(capability: impl Into<String>) -> Self {
        Self {
            capability: capability.into(),
            required: false,
            cardinality: Cardinality::ZeroOrOne,
            ..Self::default()
        }
    }

    #[inline]
    pub fn from_static(spec: RuntimeUnitRequirementSpec) -> Self {
        Self {
            capability: spec.capability.to_owned(),
            required: spec.strength.is_required(),
            cardinality: spec.cardinality,
            required_tags: spec
                .required_tags
                .iter()
                .map(|tag| tag.as_str().to_owned())
                .collect(),
            preferred_tags: spec
                .preferred_tags
                .iter()
                .map(|tag| tag.as_str().to_owned())
                .collect(),
            forbidden_tags: spec
                .forbidden_tags
                .iter()
                .map(|tag| tag.as_str().to_owned())
                .collect(),
        }
    }

    #[inline]
    pub fn strength(&self) -> RequirementStrength {
        if self.required {
            RequirementStrength::Required
        } else {
            RequirementStrength::Optional
        }
    }

    #[inline]
    pub fn with_cardinality(mut self, cardinality: Cardinality) -> Self {
        self.cardinality = cardinality;
        self
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.capability.trim().is_empty() {
            return Err("runtime-unit requirement capability must not be empty".to_owned());
        }
        for (name, tags) in [
            ("required_tags", &self.required_tags),
            ("preferred_tags", &self.preferred_tags),
            ("forbidden_tags", &self.forbidden_tags),
        ] {
            let mut seen = std::collections::BTreeSet::new();
            for tag in tags {
                let tag = tag.trim();
                if tag.is_empty() {
                    return Err(format!(
                        "runtime-unit requirement '{}' contains an empty {name} entry",
                        self.capability
                    ));
                }
                if !seen.insert(tag) {
                    return Err(format!(
                        "runtime-unit requirement '{}' contains duplicate {name} tag '{tag}'",
                        self.capability
                    ));
                }
            }
        }
        Ok(())
    }
}

impl From<RuntimeUnitRequirementSpec> for RuntimeUnitRequirementDescriptor {
    #[inline]
    fn from(spec: RuntimeUnitRequirementSpec) -> Self {
        Self::from_static(spec)
    }
}

/// Pure data description of the engine shape requested from the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EngineCompositionSpec {
    pub schema_version: u32,
    pub id: &'static str,
    pub requirements: &'static [CapabilityRequirement],
    /// Profile/game runtime-unit inventory contributed to the global unit catalog.
    /// These entries are candidates, not an imperative activation list: the host merges
    /// them with distribution and plugin inventories, then the solver selects units from
    /// the combined catalog using composition requirements and unit dependencies.
    pub runtime_units: &'static [EngineRuntimeUnitSpec],
    /// Runtime-unit-only capability roots. Unlike `requirements`, these never become
    /// provider/gateway requirements; they only drive selection inside the merged unit inventory.
    pub runtime_unit_requirements: &'static [RuntimeUnitRequirementSpec],
    /// Composition-wide soft preferences applied to every provider candidate,
    /// including routes not explicitly named by a capability requirement.
    pub preferred_tags: &'static [SystemTag],
    /// Composition-wide hard conflicts applied to every provider candidate.
    pub forbidden_tags: &'static [SystemTag],
}

impl EngineCompositionSpec {
    pub const SCHEMA_VERSION: u32 = 6;

    #[inline]
    pub const fn new(id: &'static str, requirements: &'static [CapabilityRequirement]) -> Self {
        Self {
            schema_version: Self::SCHEMA_VERSION,
            id,
            requirements,
            runtime_units: &[],
            runtime_unit_requirements: &[],
            preferred_tags: &[],
            forbidden_tags: &[],
        }
    }

    #[inline]
    pub const fn with_runtime_units(
        mut self,
        runtime_units: &'static [EngineRuntimeUnitSpec],
    ) -> Self {
        self.runtime_units = runtime_units;
        self
    }

    #[inline]
    pub const fn with_runtime_unit_requirements(
        mut self,
        requirements: &'static [RuntimeUnitRequirementSpec],
    ) -> Self {
        self.runtime_unit_requirements = requirements;
        self
    }

    #[inline]
    pub const fn with_preferred_tags(mut self, tags: &'static [SystemTag]) -> Self {
        self.preferred_tags = tags;
        self
    }

    #[inline]
    pub const fn with_forbidden_tags(mut self, tags: &'static [SystemTag]) -> Self {
        self.forbidden_tags = tags;
        self
    }
}

#[cfg(test)]
mod tests {
    include!("composition/tests.rs");
}
