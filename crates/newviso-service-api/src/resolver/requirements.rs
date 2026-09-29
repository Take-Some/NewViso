use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompositionRequirement {
    pub capability_id: String,
    pub gateway_id: String,
    pub service_kind: String,
    pub level: CapabilityRequirementLevel,
    pub min_capability_version: u32,
    pub max_capability_version: Option<u32>,
    pub contract_id: Option<String>,
    pub min_contract_version: u32,
    pub max_contract_version: Option<u32>,
    pub required_tags: Vec<String>,
    pub preferred_tags: Vec<String>,
    pub conflict_tags: Vec<String>,
    pub fallback_provider_ids: Vec<String>,
    pub min_cardinality: u16,
    pub max_cardinality: u16,
    pub declared_by: String,
}

impl CompositionRequirement {
    pub fn from_spec(
        spec: &EngineCapabilityRequirementSpec,
        declared_by: impl Into<String>,
    ) -> Self {
        let min_cardinality = spec.cardinality.min(spec.strength);
        Self {
            capability_id: spec.capability.as_str().to_owned(),
            gateway_id: spec.capability.gateway_id().to_owned(),
            service_kind: spec.capability.service_kind().to_owned(),
            level: spec.strength,
            min_capability_version: 0,
            max_capability_version: None,
            contract_id: spec
                .contract
                .map(|contract| contract.contract_id.to_owned()),
            min_contract_version: spec
                .contract
                .map(|contract| contract.min_version)
                .unwrap_or(0),
            max_contract_version: spec.contract.and_then(|contract| contract.max_version),
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
            conflict_tags: spec
                .forbidden_tags
                .iter()
                .map(|tag| tag.as_str().to_owned())
                .collect(),
            fallback_provider_ids: spec
                .fallback
                .providers()
                .iter()
                .map(|provider| (*provider).to_owned())
                .collect(),
            min_cardinality,
            max_cardinality: spec.cardinality.max().max(min_cardinality),
            declared_by: declared_by.into(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CapabilityMatrix {
    /// Backward-compatible gateway aggregate used by existing runtime callers for
    /// cardinality/fallback policy.
    requirements: Vec<CompositionRequirement>,
    /// Lossless typed requirements, merged only when both gateway and capability
    /// identity match. The solver uses this set for compatibility filtering.
    capability_requirements: Vec<CompositionRequirement>,
    preferred_tags: Vec<String>,
    conflict_tags: Vec<String>,
}

impl CapabilityMatrix {
    pub fn from_specs(
        declared_by: impl Into<String>,
        specs: &[EngineCapabilityRequirementSpec],
    ) -> Self {
        let declared_by = declared_by.into();
        Self::new(
            specs
                .iter()
                .map(|spec| CompositionRequirement::from_spec(spec, declared_by.clone()))
                .collect(),
        )
    }

    pub fn from_composition(composition: &crate::EngineCompositionSpec) -> Self {
        Self::from_specs(composition.id, composition.requirements)
            .with_preferred_tags(composition.preferred_tags.iter().map(|tag| tag.as_str()))
            .with_forbidden_tags(composition.forbidden_tags.iter().map(|tag| tag.as_str()))
    }

    pub fn new(requirements: Vec<CompositionRequirement>) -> Self {
        let mut by_capability = BTreeMap::<(String, String), CompositionRequirement>::new();
        for requirement in requirements {
            let key = (
                requirement.gateway_id.clone(),
                requirement.capability_id.clone(),
            );
            match by_capability.get_mut(&key) {
                None => {
                    by_capability.insert(key, requirement);
                }
                Some(existing) => merge_requirement(existing, requirement),
            }
        }
        let capability_requirements = by_capability.into_values().collect::<Vec<_>>();

        // Preserve the historical one-entry-per-gateway view for callers that own
        // gateway slots. The authoritative solver filtering below uses the lossless
        // capability_requirements collection instead.
        let mut by_gateway = BTreeMap::<String, CompositionRequirement>::new();
        for requirement in capability_requirements.iter().cloned() {
            match by_gateway.get_mut(&requirement.gateway_id) {
                None => {
                    by_gateway.insert(requirement.gateway_id.clone(), requirement);
                }
                Some(existing) => merge_requirement(existing, requirement),
            }
        }
        Self {
            requirements: by_gateway.into_values().collect(),
            capability_requirements,
            preferred_tags: Vec::new(),
            conflict_tags: Vec::new(),
        }
    }

    pub fn with_preferred_tags<I, S>(mut self, tags: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.preferred_tags.extend(tags.into_iter().map(Into::into));
        self.preferred_tags.sort();
        self.preferred_tags.dedup();
        self
    }

    pub fn with_forbidden_tags<I, S>(mut self, tags: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.conflict_tags.extend(tags.into_iter().map(Into::into));
        self.conflict_tags.sort();
        self.conflict_tags.dedup();
        self
    }

    #[inline]
    pub fn requirement(&self, gateway_id: &str) -> Option<&CompositionRequirement> {
        self.requirements
            .binary_search_by(|requirement| requirement.gateway_id.as_str().cmp(gateway_id))
            .ok()
            .map(|index| &self.requirements[index])
    }

    #[inline]
    pub fn requirements(&self) -> &[CompositionRequirement] {
        &self.requirements
    }

    /// Lossless typed requirements before gateway aggregation. Multiple capability
    /// identities may intentionally target the same gateway.
    #[inline]
    pub fn capability_requirements(&self) -> &[CompositionRequirement] {
        &self.capability_requirements
    }

    pub fn requirements_for_gateway(&self, gateway_id: &str) -> Vec<&CompositionRequirement> {
        self.capability_requirements_for_gateway(gateway_id)
            .iter()
            .collect()
    }

    /// Zero-allocation gateway view for the solver hot path. `CapabilityMatrix::new`
    /// stores requirements in `(gateway_id, capability_id)` order.
    pub(crate) fn capability_requirements_for_gateway(
        &self,
        gateway_id: &str,
    ) -> &[CompositionRequirement] {
        let start = self
            .capability_requirements
            .partition_point(|requirement| requirement.gateway_id.as_str() < gateway_id);
        let end = self
            .capability_requirements
            .partition_point(|requirement| requirement.gateway_id.as_str() <= gateway_id);
        &self.capability_requirements[start..end]
    }

    #[inline]
    pub fn preferred_tags(&self) -> &[String] {
        &self.preferred_tags
    }

    #[inline]
    pub fn conflict_tags(&self) -> &[String] {
        &self.conflict_tags
    }

    #[inline]
    pub fn allows_system_tags(&self, tags: &[String]) -> bool {
        !self
            .conflict_tags
            .iter()
            .any(|conflict| tags.iter().any(|tag| tag == conflict))
    }

    #[inline]
    pub fn preferred_system_tag_matches(&self, tags: &[String]) -> usize {
        self.preferred_tags
            .iter()
            .filter(|preferred| tags.iter().any(|tag| tag == *preferred))
            .count()
    }
}

fn merge_requirement(existing: &mut CompositionRequirement, incoming: CompositionRequirement) {
    existing.level = existing.level.max(incoming.level);
    existing.min_cardinality = existing.min_cardinality.max(incoming.min_cardinality);
    existing.max_cardinality = existing
        .max_cardinality
        .min(incoming.max_cardinality)
        .max(existing.min_cardinality);

    existing.min_capability_version = existing
        .min_capability_version
        .max(incoming.min_capability_version);
    existing.max_capability_version = match (
        existing.max_capability_version,
        incoming.max_capability_version,
    ) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    };

    match (&existing.contract_id, &incoming.contract_id) {
        (None, Some(_)) => {
            existing.contract_id = incoming.contract_id.clone();
            existing.min_contract_version = incoming.min_contract_version;
            existing.max_contract_version = incoming.max_contract_version;
        }
        (Some(a), Some(b)) if a == b => {
            existing.min_contract_version = existing
                .min_contract_version
                .max(incoming.min_contract_version);
            existing.max_contract_version =
                match (existing.max_contract_version, incoming.max_contract_version) {
                    (Some(a), Some(b)) => Some(a.min(b)),
                    (Some(a), None) => Some(a),
                    (None, Some(b)) => Some(b),
                    (None, None) => None,
                };
        }
        (Some(a), Some(b)) if a != b => {
            existing.contract_id = Some(format!("__conflict__:{a}|{b}"));
            existing.min_contract_version = u32::MAX;
            existing.max_contract_version = Some(0);
        }
        _ => {}
    }

    merge_unique(&mut existing.required_tags, &incoming.required_tags);
    merge_unique(&mut existing.preferred_tags, &incoming.preferred_tags);
    merge_unique(&mut existing.conflict_tags, &incoming.conflict_tags);
    merge_unique(
        &mut existing.fallback_provider_ids,
        &incoming.fallback_provider_ids,
    );
    if existing.declared_by != incoming.declared_by {
        existing.declared_by = format!("{}+{}", existing.declared_by, incoming.declared_by);
    }
}

fn merge_unique(target: &mut Vec<String>, source: &[String]) {
    target.extend(source.iter().cloned());
    target.sort();
    target.dedup();
}
