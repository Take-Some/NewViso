use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    CompositionCandidateExplanation, CompositionContractResolution,
    CompositionContractResolutionSubject, CompositionPlan, CompositionRequirement,
    RequirementStrength,
};

pub const COMPOSITION_SNAPSHOT_SCHEMA_V1: &str = "composition.snapshot_v1";
pub const COMPOSITION_DIFF_SCHEMA_V1: &str = "composition.diff_v1";
pub const COMPOSITION_OBSERVABILITY_SCHEMA_VERSION: u32 = 1;

pub const COMPOSITION_SNAPSHOT_CONTRACT_SPEC: newviso_contract_api::ContractSpec =
    newviso_contract_api::ContractSpec::new(
        "composition.snapshot.protocol",
        newviso_contract_api::ContractKind::Protocol,
        newviso_contract_api::ContractVersion::major(1),
        newviso_contract_api::ContractCompatibility::Exact,
        "newengine-service-api",
        Some(COMPOSITION_SNAPSHOT_SCHEMA_V1),
    );

pub const COMPOSITION_DIFF_CONTRACT_SPEC: newviso_contract_api::ContractSpec =
    newviso_contract_api::ContractSpec::new(
        "composition.diff.protocol",
        newviso_contract_api::ContractKind::Protocol,
        newviso_contract_api::ContractVersion::major(1),
        newviso_contract_api::ContractCompatibility::Exact,
        "newengine-service-api",
        Some(COMPOSITION_DIFF_SCHEMA_V1),
    );

pub mod composition_observability_method {
    pub const SNAPSHOT_V1: &str = "composition.snapshot_v1";
    pub const DIFF_V1: &str = "composition.diff_v1";
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompositionPlanModeV1 {
    Frozen,
    Live,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompositionSnapshotProvenanceV1 {
    pub mode: CompositionPlanModeV1,
    pub source: String,
}

impl CompositionSnapshotProvenanceV1 {
    pub fn frozen(source: impl Into<String>) -> Self {
        Self {
            mode: CompositionPlanModeV1::Frozen,
            source: source.into(),
        }
    }

    pub fn live(source: impl Into<String>) -> Self {
        Self {
            mode: CompositionPlanModeV1::Live,
            source: source.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompositionRequirementSnapshotV1 {
    pub capability_id: String,
    pub service_kind: String,
    pub level: String,
    pub min_capability_version: u32,
    pub max_capability_version: Option<u32>,
    pub contract_id: Option<String>,
    pub min_contract_version: u32,
    pub max_contract_version: Option<u32>,
    pub required_tags: Vec<String>,
    pub preferred_tags: Vec<String>,
    pub forbidden_tags: Vec<String>,
    pub fallback_provider_ids: Vec<String>,
    pub min_cardinality: u16,
    pub max_cardinality: u16,
    pub declared_by: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompositionRequirementEvaluationSnapshotV1 {
    pub capability_id: String,
    pub accepted: bool,
    pub rejection_reasons: Vec<CompositionRejectionSnapshotV1>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CompositionRejectionSnapshotV1 {
    pub code: String,
    pub capability_id: Option<String>,
    pub expected: Option<String>,
    pub actual: Option<String>,
    pub summary: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompositionScoreSnapshotV1 {
    pub origin_bias: i64,
    pub backend_priority: i32,
    pub base_preference_bonus: i64,
    pub preferred_tag_matches: Vec<String>,
    pub preferred_tag_bonus: i64,
    pub total: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompositionCandidateSnapshotV1 {
    pub candidate_id: String,
    pub provider_owner_id: String,
    pub disposition: String,
    pub rank: Option<u32>,
    pub outranked_by: Vec<String>,
    pub fallback: bool,
    pub score: CompositionScoreSnapshotV1,
    pub requirement_evaluations: Vec<CompositionRequirementEvaluationSnapshotV1>,
    pub rejection_reasons: Vec<CompositionRejectionSnapshotV1>,
    pub summary: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompositionGatewaySnapshotV1 {
    pub gateway_id: String,
    pub requirements: Vec<CompositionRequirementSnapshotV1>,
    pub candidates: Vec<CompositionCandidateSnapshotV1>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CompositionContractResolutionSnapshotV1 {
    pub subject: String,
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompositionUnsatisfiedSnapshotV1 {
    pub gateway_id: String,
    pub level: String,
    pub required_min: u16,
    pub resolved: u16,
    pub declared_by: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompositionSnapshotV1 {
    pub schema: String,
    pub schema_version: u32,
    pub instance_id: u64,
    pub composition_epoch: u64,
    pub topology_generation: u64,
    pub provenance: CompositionSnapshotProvenanceV1,
    pub gateways: Vec<CompositionGatewaySnapshotV1>,
    pub contract_resolutions: Vec<CompositionContractResolutionSnapshotV1>,
    pub unsatisfied: Vec<CompositionUnsatisfiedSnapshotV1>,
}

impl CompositionSnapshotV1 {
    pub fn from_plan(
        instance_id: u64,
        composition_epoch: u64,
        topology_generation: u64,
        provenance: CompositionSnapshotProvenanceV1,
        plan: &CompositionPlan,
    ) -> Self {
        let explanation = plan.explanation();
        let mut gateways = explanation
            .gateways()
            .iter()
            .map(|gateway| CompositionGatewaySnapshotV1 {
                gateway_id: gateway.gateway_id.clone(),
                requirements: gateway
                    .requirements
                    .iter()
                    .map(requirement_snapshot)
                    .collect(),
                candidates: gateway.candidates.iter().map(candidate_snapshot).collect(),
            })
            .collect::<Vec<_>>();
        gateways.sort_by(|a, b| a.gateway_id.cmp(&b.gateway_id));

        let mut contract_resolutions = explanation
            .contract_resolutions()
            .iter()
            .map(contract_resolution_snapshot)
            .collect::<Vec<_>>();
        contract_resolutions.sort();
        contract_resolutions.dedup();

        let mut unsatisfied = plan
            .unsatisfied()
            .iter()
            .map(|entry| CompositionUnsatisfiedSnapshotV1 {
                gateway_id: entry.gateway_id.clone(),
                level: requirement_level(entry.level).to_owned(),
                required_min: entry.required_min,
                resolved: entry.resolved,
                declared_by: entry.declared_by.clone(),
            })
            .collect::<Vec<_>>();
        unsatisfied.sort_by(|a, b| {
            a.gateway_id
                .cmp(&b.gateway_id)
                .then_with(|| a.declared_by.cmp(&b.declared_by))
        });

        Self {
            schema: COMPOSITION_SNAPSHOT_SCHEMA_V1.to_owned(),
            schema_version: COMPOSITION_OBSERVABILITY_SCHEMA_VERSION,
            instance_id,
            composition_epoch,
            topology_generation,
            provenance,
            gateways,
            contract_resolutions,
            unsatisfied,
        }
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn from_json(value: &str) -> Result<Self, String> {
        let snapshot: Self = serde_json::from_str(value).map_err(|error| error.to_string())?;
        snapshot.validate_schema()?;
        Ok(snapshot)
    }

    pub fn validate_schema(&self) -> Result<(), String> {
        if self.schema != COMPOSITION_SNAPSHOT_SCHEMA_V1 {
            return Err(format!(
                "unsupported composition snapshot schema '{}'",
                self.schema
            ));
        }
        if self.schema_version != COMPOSITION_OBSERVABILITY_SCHEMA_VERSION {
            return Err(format!(
                "unsupported composition snapshot schema version {}",
                self.schema_version
            ));
        }
        if self.topology_generation & 1 != 0 {
            return Err(format!(
                "composition.snapshot_v1 requires a stable even topology_generation, got {}",
                self.topology_generation
            ));
        }
        if self.composition_epoch != self.topology_generation / 2 {
            return Err(format!(
                "composition.snapshot_v1 epoch/generation mismatch epoch={} topology_generation={}",
                self.composition_epoch, self.topology_generation
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompositionCandidateChangeV1 {
    pub candidate_id: String,
    pub before_provider_owner_id: Option<String>,
    pub after_provider_owner_id: Option<String>,
    pub before_disposition: Option<String>,
    pub after_disposition: Option<String>,
    pub before_rank: Option<u32>,
    pub after_rank: Option<u32>,
    pub before_outranked_by: Vec<String>,
    pub after_outranked_by: Vec<String>,
    pub before_fallback: Option<bool>,
    pub after_fallback: Option<bool>,
    pub before_score: Option<CompositionScoreSnapshotV1>,
    pub after_score: Option<CompositionScoreSnapshotV1>,
    pub before_requirement_evaluations: Vec<CompositionRequirementEvaluationSnapshotV1>,
    pub after_requirement_evaluations: Vec<CompositionRequirementEvaluationSnapshotV1>,
    pub added_reasons: Vec<CompositionRejectionSnapshotV1>,
    pub removed_reasons: Vec<CompositionRejectionSnapshotV1>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompositionGatewayDiffV1 {
    pub gateway_id: String,
    pub requirements_changed: bool,
    pub before_requirements: Vec<CompositionRequirementSnapshotV1>,
    pub after_requirements: Vec<CompositionRequirementSnapshotV1>,
    pub added_candidates: Vec<String>,
    pub removed_candidates: Vec<String>,
    pub candidate_changes: Vec<CompositionCandidateChangeV1>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompositionDiffV1 {
    pub schema: String,
    pub schema_version: u32,
    pub same_instance: bool,
    pub from_instance_id: u64,
    pub to_instance_id: u64,
    pub from_epoch: u64,
    pub to_epoch: u64,
    pub from_topology_generation: u64,
    pub to_topology_generation: u64,
    pub provenance_changed: bool,
    pub from_provenance: CompositionSnapshotProvenanceV1,
    pub to_provenance: CompositionSnapshotProvenanceV1,
    pub gateway_changes: Vec<CompositionGatewayDiffV1>,
    pub added_contract_resolutions: Vec<CompositionContractResolutionSnapshotV1>,
    pub removed_contract_resolutions: Vec<CompositionContractResolutionSnapshotV1>,
    pub before_unsatisfied: Vec<CompositionUnsatisfiedSnapshotV1>,
    pub after_unsatisfied: Vec<CompositionUnsatisfiedSnapshotV1>,
}

impl CompositionDiffV1 {
    pub fn between(
        before: &CompositionSnapshotV1,
        after: &CompositionSnapshotV1,
    ) -> Result<Self, String> {
        before.validate_schema()?;
        after.validate_schema()?;

        let before_gateways = before
            .gateways
            .iter()
            .map(|gateway| (gateway.gateway_id.as_str(), gateway))
            .collect::<BTreeMap<_, _>>();
        let after_gateways = after
            .gateways
            .iter()
            .map(|gateway| (gateway.gateway_id.as_str(), gateway))
            .collect::<BTreeMap<_, _>>();
        let gateway_ids = before_gateways
            .keys()
            .chain(after_gateways.keys())
            .copied()
            .collect::<BTreeSet<_>>();

        let mut gateway_changes = Vec::new();
        for gateway_id in gateway_ids {
            let before_gateway = before_gateways.get(gateway_id).copied();
            let after_gateway = after_gateways.get(gateway_id).copied();
            let before_requirements = before_gateway
                .map(|gateway| gateway.requirements.clone())
                .unwrap_or_default();
            let after_requirements = after_gateway
                .map(|gateway| gateway.requirements.clone())
                .unwrap_or_default();
            let requirements_changed = before_requirements != after_requirements;

            let before_candidates = candidate_map(before_gateway);
            let after_candidates = candidate_map(after_gateway);
            let candidate_ids = before_candidates
                .keys()
                .chain(after_candidates.keys())
                .copied()
                .collect::<BTreeSet<_>>();
            let mut added_candidates = Vec::new();
            let mut removed_candidates = Vec::new();
            let mut candidate_changes = Vec::new();

            for candidate_id in candidate_ids {
                let old = before_candidates.get(candidate_id).copied();
                let new = after_candidates.get(candidate_id).copied();
                match (old, new) {
                    (None, Some(_)) => added_candidates.push(candidate_id.to_owned()),
                    (Some(_), None) => removed_candidates.push(candidate_id.to_owned()),
                    _ => {}
                }
                if !candidate_semantically_equal(old, new) {
                    candidate_changes.push(candidate_change(candidate_id, old, new));
                }
            }

            if requirements_changed
                || !added_candidates.is_empty()
                || !removed_candidates.is_empty()
                || !candidate_changes.is_empty()
            {
                gateway_changes.push(CompositionGatewayDiffV1 {
                    gateway_id: gateway_id.to_owned(),
                    requirements_changed,
                    before_requirements,
                    after_requirements,
                    added_candidates,
                    removed_candidates,
                    candidate_changes,
                });
            }
        }

        let before_contracts = before
            .contract_resolutions
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        let after_contracts = after
            .contract_resolutions
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();

        Ok(Self {
            schema: COMPOSITION_DIFF_SCHEMA_V1.to_owned(),
            schema_version: COMPOSITION_OBSERVABILITY_SCHEMA_VERSION,
            same_instance: before.instance_id == after.instance_id,
            from_instance_id: before.instance_id,
            to_instance_id: after.instance_id,
            from_epoch: before.composition_epoch,
            to_epoch: after.composition_epoch,
            from_topology_generation: before.topology_generation,
            to_topology_generation: after.topology_generation,
            provenance_changed: before.provenance != after.provenance,
            from_provenance: before.provenance.clone(),
            to_provenance: after.provenance.clone(),
            gateway_changes,
            added_contract_resolutions: after_contracts
                .difference(&before_contracts)
                .cloned()
                .collect(),
            removed_contract_resolutions: before_contracts
                .difference(&after_contracts)
                .cloned()
                .collect(),
            before_unsatisfied: before.unsatisfied.clone(),
            after_unsatisfied: after.unsatisfied.clone(),
        })
    }

    pub fn is_empty(&self) -> bool {
        self.same_instance
            && !self.provenance_changed
            && self.gateway_changes.is_empty()
            && self.added_contract_resolutions.is_empty()
            && self.removed_contract_resolutions.is_empty()
            && self.before_unsatisfied == self.after_unsatisfied
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn from_json(value: &str) -> Result<Self, String> {
        let diff: Self = serde_json::from_str(value).map_err(|error| error.to_string())?;
        if diff.schema != COMPOSITION_DIFF_SCHEMA_V1
            || diff.schema_version != COMPOSITION_OBSERVABILITY_SCHEMA_VERSION
        {
            return Err(format!(
                "unsupported composition diff schema '{}' version {}",
                diff.schema, diff.schema_version
            ));
        }
        Ok(diff)
    }
}

fn requirement_snapshot(requirement: &CompositionRequirement) -> CompositionRequirementSnapshotV1 {
    CompositionRequirementSnapshotV1 {
        capability_id: requirement.capability_id.clone(),
        service_kind: requirement.service_kind.clone(),
        level: requirement_level(requirement.level).to_owned(),
        min_capability_version: requirement.min_capability_version,
        max_capability_version: requirement.max_capability_version,
        contract_id: requirement.contract_id.clone(),
        min_contract_version: requirement.min_contract_version,
        max_contract_version: requirement.max_contract_version,
        required_tags: requirement.required_tags.clone(),
        preferred_tags: requirement.preferred_tags.clone(),
        forbidden_tags: requirement.conflict_tags.clone(),
        fallback_provider_ids: requirement.fallback_provider_ids.clone(),
        min_cardinality: requirement.min_cardinality,
        max_cardinality: requirement.max_cardinality,
        declared_by: requirement.declared_by.clone(),
    }
}

fn candidate_snapshot(
    candidate: &CompositionCandidateExplanation,
) -> CompositionCandidateSnapshotV1 {
    CompositionCandidateSnapshotV1 {
        candidate_id: candidate.candidate_id.clone(),
        provider_owner_id: candidate.provider_owner_id.clone(),
        disposition: candidate.disposition.as_str().to_owned(),
        rank: candidate
            .rank
            .map(|rank| rank.min(u32::MAX as usize) as u32),
        outranked_by: candidate.outranked_by.clone(),
        fallback: candidate.fallback,
        score: CompositionScoreSnapshotV1 {
            origin_bias: candidate.score.origin_bias,
            backend_priority: candidate.score.backend_priority,
            base_preference_bonus: candidate.score.base_preference_bonus,
            preferred_tag_matches: candidate.score.preferred_tag_matches.clone(),
            preferred_tag_bonus: candidate.score.preferred_tag_bonus,
            total: candidate.score.total,
        },
        requirement_evaluations: candidate
            .requirement_evaluations
            .iter()
            .map(|evaluation| CompositionRequirementEvaluationSnapshotV1 {
                capability_id: evaluation.capability_id.clone(),
                accepted: evaluation.accepted,
                rejection_reasons: evaluation
                    .rejection_reasons
                    .iter()
                    .map(|reason| CompositionRejectionSnapshotV1 {
                        code: reason.code().to_owned(),
                        capability_id: reason.capability_id.clone(),
                        expected: reason.expected.clone(),
                        actual: reason.actual.clone(),
                        summary: reason.summary(),
                    })
                    .collect(),
            })
            .collect(),
        rejection_reasons: candidate
            .rejection_reasons
            .iter()
            .map(|reason| CompositionRejectionSnapshotV1 {
                code: reason.code().to_owned(),
                capability_id: reason.capability_id.clone(),
                expected: reason.expected.clone(),
                actual: reason.actual.clone(),
                summary: reason.summary(),
            })
            .collect(),
        summary: candidate.summary(),
    }
}

fn contract_resolution_snapshot(
    resolution: &CompositionContractResolution,
) -> CompositionContractResolutionSnapshotV1 {
    CompositionContractResolutionSnapshotV1 {
        subject: match resolution.subject {
            CompositionContractResolutionSubject::Candidate => "candidate",
            CompositionContractResolutionSubject::Requirement => "requirement",
        }
        .to_owned(),
        gateway_id: resolution.gateway_id.clone(),
        candidate_id: resolution.candidate_id.clone(),
        capability_id: resolution.capability_id.clone(),
        reference: resolution.reference.clone(),
        canonical_id: resolution.canonical_id.clone(),
        min_version: resolution.min_version,
        max_version: resolution.max_version,
        authority: resolution.authority.clone(),
        owner: resolution.owner.clone(),
    }
}

fn requirement_level(level: RequirementStrength) -> &'static str {
    match level {
        RequirementStrength::Optional => "optional",
        RequirementStrength::Preferred => "preferred",
        RequirementStrength::Required => "required",
    }
}

fn candidate_map(
    gateway: Option<&CompositionGatewaySnapshotV1>,
) -> BTreeMap<&str, &CompositionCandidateSnapshotV1> {
    gateway
        .into_iter()
        .flat_map(|gateway| gateway.candidates.iter())
        .map(|candidate| (candidate.candidate_id.as_str(), candidate))
        .collect()
}

fn candidate_change(
    candidate_id: &str,
    before: Option<&CompositionCandidateSnapshotV1>,
    after: Option<&CompositionCandidateSnapshotV1>,
) -> CompositionCandidateChangeV1 {
    let before_reasons = before
        .into_iter()
        .flat_map(|candidate| candidate.rejection_reasons.iter().cloned())
        .collect::<BTreeSet<_>>();
    let after_reasons = after
        .into_iter()
        .flat_map(|candidate| candidate.rejection_reasons.iter().cloned())
        .collect::<BTreeSet<_>>();
    CompositionCandidateChangeV1 {
        candidate_id: candidate_id.to_owned(),
        before_provider_owner_id: before.map(|candidate| candidate.provider_owner_id.clone()),
        after_provider_owner_id: after.map(|candidate| candidate.provider_owner_id.clone()),
        before_disposition: before.map(|candidate| candidate.disposition.clone()),
        after_disposition: after.map(|candidate| candidate.disposition.clone()),
        before_rank: before.and_then(|candidate| candidate.rank),
        after_rank: after.and_then(|candidate| candidate.rank),
        before_outranked_by: before
            .map(|candidate| candidate.outranked_by.clone())
            .unwrap_or_default(),
        after_outranked_by: after
            .map(|candidate| candidate.outranked_by.clone())
            .unwrap_or_default(),
        before_fallback: before.map(|candidate| candidate.fallback),
        after_fallback: after.map(|candidate| candidate.fallback),
        before_score: before.map(|candidate| candidate.score.clone()),
        after_score: after.map(|candidate| candidate.score.clone()),
        before_requirement_evaluations: before
            .map(|candidate| candidate.requirement_evaluations.clone())
            .unwrap_or_default(),
        after_requirement_evaluations: after
            .map(|candidate| candidate.requirement_evaluations.clone())
            .unwrap_or_default(),
        added_reasons: after_reasons.difference(&before_reasons).cloned().collect(),
        removed_reasons: before_reasons.difference(&after_reasons).cloned().collect(),
    }
}

fn candidate_semantically_equal(
    before: Option<&CompositionCandidateSnapshotV1>,
    after: Option<&CompositionCandidateSnapshotV1>,
) -> bool {
    match (before, after) {
        (None, None) => true,
        (Some(before), Some(after)) => {
            before.candidate_id == after.candidate_id
                && before.provider_owner_id == after.provider_owner_id
                && before.disposition == after.disposition
                && before.rank == after.rank
                && before.outranked_by == after.outranked_by
                && before.fallback == after.fallback
                && before.score == after.score
                && before.requirement_evaluations == after.requirement_evaluations
                && before.rejection_reasons == after.rejection_reasons
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    include!("observability/tests.rs");
}
