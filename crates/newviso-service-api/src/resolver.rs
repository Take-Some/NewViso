use std::collections::{BTreeMap, BTreeSet};

use crate::{CapabilityRequirementLevel, EngineCapabilityRequirementSpec};

const PREFERRED_TAG_BONUS: i64 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompositionCandidateDisposition {
    Selected,
    Shadowed,
    Rejected,
}

impl CompositionCandidateDisposition {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Selected => "selected",
            Self::Shadowed => "shadowed",
            Self::Rejected => "rejected",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CompositionRejectionKind {
    FormatMismatch,
    ExternalPolicy,
    CompositionForbiddenTag,
    MissingCapability,
    MissingCapabilityVersion,
    CapabilityVersionBelowMinimum,
    CapabilityVersionAboveMaximum,
    ContractMismatch,
    MissingContractVersion,
    ContractVersionBelowMinimum,
    ContractVersionAboveMaximum,
    MissingRequiredTag,
    ForbiddenTag,
    FallbackSuppressed,
}

impl CompositionRejectionKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FormatMismatch => "format_mismatch",
            Self::ExternalPolicy => "external_policy",
            Self::CompositionForbiddenTag => "composition_forbidden_tag",
            Self::MissingCapability => "missing_capability",
            Self::MissingCapabilityVersion => "missing_capability_version",
            Self::CapabilityVersionBelowMinimum => "capability_version_below_minimum",
            Self::CapabilityVersionAboveMaximum => "capability_version_above_maximum",
            Self::ContractMismatch => "contract_mismatch",
            Self::MissingContractVersion => "missing_contract_version",
            Self::ContractVersionBelowMinimum => "contract_version_below_minimum",
            Self::ContractVersionAboveMaximum => "contract_version_above_maximum",
            Self::MissingRequiredTag => "missing_required_tag",
            Self::ForbiddenTag => "forbidden_tag",
            Self::FallbackSuppressed => "fallback_suppressed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct CompositionRejectionReason {
    pub kind: CompositionRejectionKind,
    pub capability_id: Option<String>,
    pub expected: Option<String>,
    pub actual: Option<String>,
}

impl CompositionRejectionReason {
    pub fn new(kind: CompositionRejectionKind) -> Self {
        Self {
            kind,
            capability_id: None,
            expected: None,
            actual: None,
        }
    }

    pub fn for_capability(mut self, capability_id: impl Into<String>) -> Self {
        self.capability_id = Some(capability_id.into());
        self
    }

    pub fn with_expected(mut self, expected: impl Into<String>) -> Self {
        self.expected = Some(expected.into());
        self
    }

    pub fn with_actual(mut self, actual: impl Into<String>) -> Self {
        self.actual = Some(actual.into());
        self
    }

    pub const fn code(&self) -> &'static str {
        self.kind.as_str()
    }

    pub fn summary(&self) -> String {
        let mut out = self.kind.as_str().to_owned();
        if let Some(capability_id) = self.capability_id.as_deref() {
            out.push_str(" capability=");
            out.push_str(capability_id);
        }
        if let Some(expected) = self.expected.as_deref() {
            out.push_str(" expected=");
            out.push_str(expected);
        }
        if let Some(actual) = self.actual.as_deref() {
            out.push_str(" actual=");
            out.push_str(actual);
        }
        out
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompositionRequirementEvaluation {
    pub capability_id: String,
    pub accepted: bool,
    pub rejection_reasons: Vec<CompositionRejectionReason>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CompositionScoreBreakdown {
    pub origin_bias: i64,
    pub backend_priority: i32,
    pub base_preference_bonus: i64,
    pub preferred_tag_matches: Vec<String>,
    pub preferred_tag_bonus: i64,
    pub total: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompositionCandidateExplanation {
    pub gateway_id: String,
    pub candidate_id: String,
    pub provider_owner_id: String,
    pub disposition: CompositionCandidateDisposition,
    /// One-based rank among compatible candidates after shared scoring/tie-breakers.
    /// Rejected candidates have no rank.
    pub rank: Option<usize>,
    /// Selected candidates that outrank this compatible shadowed candidate.
    pub outranked_by: Vec<String>,
    pub fallback: bool,
    pub score: CompositionScoreBreakdown,
    pub requirement_evaluations: Vec<CompositionRequirementEvaluation>,
    pub rejection_reasons: Vec<CompositionRejectionReason>,
}

impl CompositionCandidateExplanation {
    pub fn summary(&self) -> String {
        let preferred = if self.score.preferred_tag_matches.is_empty() {
            "none".to_owned()
        } else {
            self.score.preferred_tag_matches.join(",")
        };
        let rank = self
            .rank
            .map(|rank| rank.to_string())
            .unwrap_or_else(|| "none".to_owned());
        let outranked_by = if self.outranked_by.is_empty() {
            "none".to_owned()
        } else {
            self.outranked_by.join(",")
        };
        if self.rejection_reasons.is_empty() {
            format!(
                "{} rank={} outranked_by={} score={} origin_bias={} backend_priority={} preference_bonus={} preferred_tags={}",
                self.disposition.as_str(),
                rank,
                outranked_by,
                self.score.total,
                self.score.origin_bias,
                self.score.backend_priority,
                self.score.base_preference_bonus + self.score.preferred_tag_bonus,
                preferred
            )
        } else {
            format!(
                "{} reasons=[{}] rank={} outranked_by={} score={} origin_bias={} backend_priority={} preference_bonus={} preferred_tags={}",
                self.disposition.as_str(),
                self.rejection_reasons
                    .iter()
                    .map(CompositionRejectionReason::summary)
                    .collect::<Vec<_>>()
                    .join("; "),
                rank,
                outranked_by,
                self.score.total,
                self.score.origin_bias,
                self.score.backend_priority,
                self.score.base_preference_bonus + self.score.preferred_tag_bonus,
                preferred
            )
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayCompositionExplanation {
    pub gateway_id: String,
    pub requirements: Vec<CompositionRequirement>,
    pub candidates: Vec<CompositionCandidateExplanation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompositionContractResolutionSubject {
    Candidate,
    Requirement,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompositionContractResolution {
    pub subject: CompositionContractResolutionSubject,
    pub gateway_id: String,
    pub candidate_id: Option<String>,
    pub capability_id: String,
    pub reference: String,
    pub canonical_id: String,
    pub min_version: u32,
    pub max_version: Option<u32>,
    pub authority: String,
    pub owner: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CompositionExplanationGraph {
    gateways: Vec<GatewayCompositionExplanation>,
    contract_resolutions: Vec<CompositionContractResolution>,
}

impl CompositionExplanationGraph {
    pub fn gateway(&self, gateway_id: &str) -> Option<&GatewayCompositionExplanation> {
        self.gateways
            .binary_search_by(|entry| entry.gateway_id.as_str().cmp(gateway_id))
            .ok()
            .map(|index| &self.gateways[index])
    }

    pub fn gateways(&self) -> &[GatewayCompositionExplanation] {
        &self.gateways
    }

    pub fn contract_resolutions(&self) -> &[CompositionContractResolution] {
        &self.contract_resolutions
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompositionCapabilityMetadata {
    pub id: String,
    pub version: Option<u32>,
    pub contract_id: Option<String>,
    pub contract_version: Option<u32>,
    pub tags: Vec<String>,
}

impl CompositionCapabilityMetadata {
    #[inline]
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            version: None,
            contract_id: None,
            contract_version: None,
            tags: Vec::new(),
        }
    }

    #[inline]
    pub fn with_version(mut self, version: u32) -> Self {
        self.version = Some(version);
        self
    }

    #[inline]
    pub fn with_contract(mut self, contract_id: impl Into<String>, version: Option<u32>) -> Self {
        self.contract_id = Some(contract_id.into());
        self.contract_version = version;
        self
    }

    pub fn with_tags<I, S>(mut self, tags: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.tags = tags.into_iter().map(Into::into).collect();
        self.tags.sort();
        self.tags.dedup();
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompositionCandidate {
    pub gateway_id: String,
    pub capability_id: Option<String>,
    /// Complete capability set advertised by this provider candidate. `capability_id`
    /// remains as the V1/single-capability compatibility field.
    pub capability_ids: Vec<String>,
    /// Lossless V2 metadata for each advertised capability. Legacy callers may
    /// continue using capability_id/capability_ids + the candidate-wide fields.
    pub capability_metadata: Vec<CompositionCapabilityMetadata>,
    pub capability_version: Option<u32>,
    pub candidate_id: String,
    pub provider_owner_id: String,
    pub backend_priority: i32,
    pub origin_bias: i64,
    pub preference_bonus: i64,
    pub contract_id: Option<String>,
    pub contract_version: Option<u32>,
    pub tags: Vec<String>,
    /// Domain adapters may record non-solver eligibility failures (for example
    /// an Editor format/extension mismatch). The shared solver owns the final
    /// rejection graph and never requires consumers to reconstruct these reasons.
    pub preflight_rejections: Vec<CompositionRejectionReason>,
}

impl CompositionCandidate {
    #[inline]
    pub fn new(
        gateway_id: impl Into<String>,
        candidate_id: impl Into<String>,
        provider_owner_id: impl Into<String>,
        backend_priority: i32,
        origin_bias: i64,
        preference_bonus: i64,
    ) -> Self {
        Self {
            gateway_id: gateway_id.into(),
            capability_id: None,
            capability_ids: Vec::new(),
            capability_metadata: Vec::new(),
            capability_version: None,
            candidate_id: candidate_id.into(),
            provider_owner_id: provider_owner_id.into(),
            backend_priority,
            origin_bias,
            preference_bonus,
            contract_id: None,
            contract_version: None,
            tags: Vec::new(),
            preflight_rejections: Vec::new(),
        }
    }

    #[inline]
    pub fn with_contract_id(mut self, contract_id: impl Into<String>) -> Self {
        self.contract_id = Some(contract_id.into());
        self
    }

    #[inline]
    pub fn with_contract(mut self, contract_id: impl Into<String>, version: u32) -> Self {
        self.contract_id = Some(contract_id.into());
        self.contract_version = Some(version);
        self
    }

    #[inline]
    pub fn with_capability(mut self, capability_id: impl Into<String>) -> Self {
        let capability_id = capability_id.into();
        self.capability_id = Some(capability_id.clone());
        self.capability_ids.push(capability_id);
        self.capability_ids.sort();
        self.capability_ids.dedup();
        self
    }

    /// Advertises every capability implemented by one provider candidate. This is
    /// required when one routed provider must satisfy multiple typed requirements
    /// on the same gateway (for example an editor format provider that must read,
    /// inspect and preview the same asset type).
    pub fn with_capabilities<I, S>(mut self, capability_ids: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.capability_ids
            .extend(capability_ids.into_iter().map(Into::into));
        self.capability_ids.sort();
        self.capability_ids.dedup();
        if self.capability_id.is_none() {
            self.capability_id = self.capability_ids.first().cloned();
        }
        self
    }

    #[inline]
    pub fn with_capability_version(mut self, version: u32) -> Self {
        self.capability_version = Some(version);
        self
    }

    pub fn with_capability_metadata<I>(mut self, capabilities: I) -> Self
    where
        I: IntoIterator<Item = CompositionCapabilityMetadata>,
    {
        self.capability_metadata.extend(capabilities);
        self.capability_metadata.sort_by(|a, b| a.id.cmp(&b.id));
        self.capability_metadata.dedup_by(|a, b| a.id == b.id);
        self.capability_ids.extend(
            self.capability_metadata
                .iter()
                .map(|capability| capability.id.clone()),
        );
        self.capability_ids.sort();
        self.capability_ids.dedup();
        if self.capability_id.is_none() {
            self.capability_id = self.capability_ids.first().cloned();
        }
        self
    }

    pub fn with_tags<I, S>(mut self, tags: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.tags = tags.into_iter().map(Into::into).collect();
        self.tags.sort();
        self.tags.dedup();
        self
    }

    pub fn with_preflight_rejection(mut self, reason: CompositionRejectionReason) -> Self {
        self.preflight_rejections.push(reason);
        self.preflight_rejections.sort();
        self.preflight_rejections.dedup();
        self
    }
}

#[path = "resolver/requirements.rs"]
mod requirements;
pub use requirements::{CapabilityMatrix, CompositionRequirement};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompositionSelection {
    pub gateway_id: String,
    pub candidate_id: String,
    pub provider_owner_id: String,
    pub backend_priority: i32,
    pub origin_bias: i64,
    pub score: i64,
    pub fallback: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayCompositionPlan {
    gateway_id: String,
    selected: Vec<CompositionSelection>,
    shadowed: Vec<CompositionSelection>,
}

impl GatewayCompositionPlan {
    #[inline]
    pub fn active(&self) -> Option<&CompositionSelection> {
        self.selected.first()
    }

    #[inline]
    pub fn selected(&self) -> &[CompositionSelection] {
        &self.selected
    }

    #[inline]
    pub fn shadowed(&self) -> &[CompositionSelection] {
        &self.shadowed
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnsatisfiedCapabilityRequirement {
    pub gateway_id: String,
    pub level: CapabilityRequirementLevel,
    pub required_min: u16,
    pub resolved: u16,
    pub declared_by: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CompositionPlan {
    gateways: Vec<GatewayCompositionPlan>,
    unsatisfied: Vec<UnsatisfiedCapabilityRequirement>,
    explanation: CompositionExplanationGraph,
}

impl CompositionPlan {
    #[inline]
    pub fn selected(&self, gateway_id: &str) -> Option<&CompositionSelection> {
        self.gateway_plan(gateway_id)
            .and_then(GatewayCompositionPlan::active)
    }

    #[inline]
    pub fn selected_all(&self, gateway_id: &str) -> &[CompositionSelection] {
        self.gateway_plan(gateway_id)
            .map(GatewayCompositionPlan::selected)
            .unwrap_or(&[])
    }

    #[inline]
    pub fn gateway_plan(&self, gateway_id: &str) -> Option<&GatewayCompositionPlan> {
        self.gateways
            .binary_search_by(|entry| entry.gateway_id.as_str().cmp(gateway_id))
            .ok()
            .map(|index| &self.gateways[index])
    }

    pub fn gateway_ids(&self) -> Vec<String> {
        self.gateways
            .iter()
            .map(|entry| entry.gateway_id.clone())
            .collect()
    }

    #[inline]
    pub fn unsatisfied(&self) -> &[UnsatisfiedCapabilityRequirement] {
        &self.unsatisfied
    }

    pub fn explanation(&self) -> &CompositionExplanationGraph {
        &self.explanation
    }

    pub fn with_contract_resolutions<I>(mut self, resolutions: I) -> Self
    where
        I: IntoIterator<Item = CompositionContractResolution>,
    {
        self.explanation.contract_resolutions.extend(resolutions);
        self.explanation.contract_resolutions.sort_by(|a, b| {
            a.gateway_id
                .cmp(&b.gateway_id)
                .then_with(|| a.candidate_id.cmp(&b.candidate_id))
                .then_with(|| a.capability_id.cmp(&b.capability_id))
                .then_with(|| a.reference.cmp(&b.reference))
        });
        self.explanation.contract_resolutions.dedup();
        self
    }

    pub fn validate_required(&self) -> Result<(), String> {
        let missing = self
            .unsatisfied
            .iter()
            .filter(|missing| missing.level.is_required())
            .map(|missing| {
                format!(
                    "{}(resolved={}/{} declared_by={})",
                    missing.gateway_id, missing.resolved, missing.required_min, missing.declared_by
                )
            })
            .collect::<Vec<_>>();
        if missing.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "required capability requirement(s) are unsatisfied: {}",
                missing.join(", ")
            ))
        }
    }
}

#[path = "resolver/solver.rs"]
mod solver;
pub use solver::{parse_versioned_contract_id, CompositionSolver, CompositionSolverInput};

#[cfg(test)]
#[path = "resolver/tests.rs"]
mod tests;
