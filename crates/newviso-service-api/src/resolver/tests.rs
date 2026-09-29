use super::*;

use crate::{
    CapabilityId, CapabilityRequirement, Cardinality, FallbackPolicy, RequirementStrength,
    SystemTag,
};

const RENDER: CapabilityId = CapabilityId::new("render.backend", "engine.render", "render");
const CODEC: CapabilityId = CapabilityId::new("codec.backend", "engine.codec", "codec");
const SHADOW: SystemTag = SystemTag::new("feature.shadow");
const SOFTWARE: SystemTag = SystemTag::new("backend.software");
const TIMELINE: SystemTag = SystemTag::new("feature.timeline");

fn candidate(id: &str, priority: i32) -> CompositionCandidate {
    CompositionCandidate::new("engine.render", id, id, priority, 20_000, 0)
        .with_capability("render.backend")
}

#[test]
fn higher_score_wins() {
    let plan = CompositionSolver::resolve([candidate("low", 10), candidate("high", 20)]);
    assert_eq!(
        plan.selected("engine.render")
            .map(|selection| selection.candidate_id.as_str()),
        Some("high")
    );
}

#[test]
fn version_tags_conflicts_and_fallback_are_resolver_input() {
    const SPEC: CapabilityRequirement = CapabilityRequirement::required(RENDER)
        .with_contract("newengine.render-provider", 2, Some(3))
        .with_required_tags(&[SHADOW])
        .with_forbidden_tags(&[SOFTWARE])
        .with_fallback(FallbackPolicy::Providers(&["render.null"]));
    let matrix = CapabilityMatrix::from_specs("test", &[SPEC]);
    let compatible = candidate("render.real", 10)
        .with_contract("newengine.render-provider", 2)
        .with_tags(["feature.shadow"]);
    let incompatible = candidate("render.bad", 1000)
        .with_contract("newengine.render-provider", 1)
        .with_tags(["feature.shadow"]);
    let forbidden = candidate("render.software", 2000)
        .with_contract("newengine.render-provider", 2)
        .with_tags(["feature.shadow", "backend.software"]);
    let fallback = candidate("render.null", -1000)
        .with_contract("newengine.render-provider", 2)
        .with_tags(["feature.shadow"]);
    let plan = CompositionSolver::resolve_input(CompositionSolverInput {
        candidates: vec![fallback, incompatible, forbidden, compatible],
        capability_matrix: matrix,
    });
    assert_eq!(
        plan.selected("engine.render")
            .map(|selection| selection.candidate_id.as_str()),
        Some("render.real")
    );
    assert!(plan.validate_required().is_ok());
}

#[test]
fn required_many_requires_at_least_one_provider() {
    const SPEC: CapabilityRequirement =
        CapabilityRequirement::required(CODEC).with_cardinality(Cardinality::Many);
    let missing = CompositionSolver::resolve_input(CompositionSolverInput {
        candidates: Vec::new(),
        capability_matrix: CapabilityMatrix::from_specs("test", &[SPEC]),
    });
    assert!(missing.validate_required().is_err());

    let resolved = CompositionSolver::resolve_input(CompositionSolverInput {
        candidates: vec![
            CompositionCandidate::new("engine.codec", "codec.a", "codec.a", 0, 0, 0)
                .with_capability("codec.backend"),
            CompositionCandidate::new("engine.codec", "codec.b", "codec.b", 0, 0, 0)
                .with_capability("codec.backend"),
        ],
        capability_matrix: CapabilityMatrix::from_specs("test", &[SPEC]),
    });
    assert!(resolved.validate_required().is_ok());
    assert_eq!(resolved.selected_all("engine.codec").len(), 2);
}

#[test]
fn duplicate_requirements_merge_to_strictest_matrix() {
    let required = CompositionRequirement::from_spec(
        &CapabilityRequirement::required(RENDER).with_required_tags(&[SHADOW]),
        "game",
    );
    let preferred = CompositionRequirement::from_spec(
        &CapabilityRequirement::preferred(RENDER).with_preferred_tags(&[TIMELINE]),
        "editor",
    );
    let matrix = CapabilityMatrix::new(vec![required, preferred]);
    assert_eq!(matrix.requirements().len(), 1);
    let requirement = matrix.requirement("engine.render").unwrap();
    assert_eq!(requirement.level, RequirementStrength::Required);
    assert!(requirement
        .required_tags
        .contains(&"feature.shadow".to_owned()));
    assert!(requirement
        .preferred_tags
        .contains(&"feature.timeline".to_owned()));
}

#[test]
fn capability_version_and_typed_tag_constraints_drive_selection() {
    let mut requirement = CompositionRequirement::from_spec(
        &CapabilityRequirement::required(RENDER)
            .with_required_tags(&[SHADOW])
            .with_preferred_tags(&[TIMELINE])
            .with_forbidden_tags(&[SOFTWARE]),
        "typed-plugin",
    );
    requirement.min_capability_version = 2;
    requirement.max_capability_version = Some(3);
    let matrix = CapabilityMatrix::new(vec![requirement]);

    let preferred = candidate("render.preferred", 10)
        .with_capability_version(3)
        .with_tags(["feature.shadow", "feature.timeline"]);
    let plain = candidate("render.plain", 10)
        .with_capability_version(3)
        .with_tags(["feature.shadow"]);
    let too_new = candidate("render.too-new", 10_000)
        .with_capability_version(4)
        .with_tags(["feature.shadow", "feature.timeline"]);
    let forbidden = candidate("render.software", 20_000)
        .with_capability_version(3)
        .with_tags(["feature.shadow", "feature.timeline", "backend.software"]);

    let plan = CompositionSolver::resolve_input(CompositionSolverInput {
        candidates: vec![plain, forbidden, too_new, preferred],
        capability_matrix: matrix,
    });
    assert_eq!(
        plan.selected("engine.render")
            .map(|selection| selection.candidate_id.as_str()),
        Some("render.preferred")
    );
    assert!(plan.validate_required().is_ok());
}

#[test]
fn composition_wide_forbidden_tags_filter_candidates_without_gateway_requirement() {
    let headful = CompositionCandidate::new(
        "vendor.presentation",
        "vendor.alpha",
        "vendor.alpha",
        100,
        0,
        0,
    )
    .with_tags(["headful"]);
    let headless =
        CompositionCandidate::new("vendor.presentation", "vendor.beta", "vendor.beta", 1, 0, 0)
            .with_tags(["headless", "deterministic"]);

    let plan = CompositionSolver::resolve_input(CompositionSolverInput {
        candidates: vec![headful, headless],
        capability_matrix: CapabilityMatrix::default().with_forbidden_tags(["headful"]),
    });

    assert_eq!(
        plan.selected("vendor.presentation")
            .map(|selection| selection.candidate_id.as_str()),
        Some("vendor.beta")
    );
}

#[test]
fn composition_wide_preferred_tags_rank_candidates_without_provider_name_knowledge() {
    let implementation_named_like_render = CompositionCandidate::new(
        "vendor.output",
        "engine.render.vulkan",
        "engine.render.vulkan",
        10,
        0,
        0,
    );
    let tagged_headless = CompositionCandidate::new(
        "vendor.output",
        "vendor.null-output",
        "vendor.null-output",
        10,
        0,
        0,
    )
    .with_tags(["headless"]);

    let plan = CompositionSolver::resolve_input(CompositionSolverInput {
        candidates: vec![implementation_named_like_render, tagged_headless],
        capability_matrix: CapabilityMatrix::default().with_preferred_tags(["headless"]),
    });

    assert_eq!(
        plan.selected("vendor.output")
            .map(|selection| selection.candidate_id.as_str()),
        Some("vendor.null-output")
    );
}

#[test]
fn provider_name_does_not_imply_headful_policy() {
    let provider = CompositionCandidate::new(
        "vendor.output",
        "engine.render.vulkan",
        "engine.render.vulkan",
        10,
        0,
        0,
    );
    let plan = CompositionSolver::resolve_input(CompositionSolverInput {
        candidates: vec![provider],
        capability_matrix: CapabilityMatrix::default().with_forbidden_tags(["headful"]),
    });
    assert_eq!(
        plan.selected("vendor.output")
            .map(|selection| selection.candidate_id.as_str()),
        Some("engine.render.vulkan")
    );
}

#[test]
fn multiple_capabilities_on_one_gateway_require_one_provider_to_satisfy_all() {
    const READ: CapabilityId =
        CapabilityId::new("asset.format.read", "editor.preview", "editor.format");
    const PREVIEW: CapabilityId =
        CapabilityId::new("asset.preview.texture", "editor.preview", "editor.format");
    let matrix = CapabilityMatrix::from_specs(
        "editor.format.texture",
        &[
            CapabilityRequirement::required(READ),
            CapabilityRequirement::required(PREVIEW),
        ],
    );
    assert_eq!(matrix.capability_requirements().len(), 2);

    let partial = CompositionCandidate::new(
        "editor.preview",
        "provider.partial",
        "provider.partial",
        10_000,
        0,
        0,
    )
    .with_capabilities(["asset.format.read"]);
    let complete = CompositionCandidate::new(
        "editor.preview",
        "provider.complete",
        "provider.complete",
        0,
        0,
        0,
    )
    .with_capabilities(["asset.format.read", "asset.preview.texture"]);

    let plan = CompositionSolver::resolve_input(CompositionSolverInput {
        candidates: vec![partial, complete],
        capability_matrix: matrix,
    });
    assert_eq!(
        plan.selected("editor.preview")
            .map(|selection| selection.candidate_id.as_str()),
        Some("provider.complete")
    );
    assert!(plan.validate_required().is_ok());
}

#[test]
fn per_capability_v2_metadata_drives_version_contract_and_tags() {
    let mut read = CompositionRequirement::from_spec(
        &CapabilityRequirement::required(RENDER).with_required_tags(&[SHADOW]),
        "editor",
    );
    read.capability_id = "asset.format.read".to_owned();
    read.min_capability_version = 2;
    read.contract_id = Some("editor.asset.read".to_owned());
    read.min_contract_version = 3;

    let mut preview = read.clone();
    preview.capability_id = "asset.preview.texture".to_owned();
    preview.min_capability_version = 4;
    preview.contract_id = Some("editor.asset.preview".to_owned());
    preview.min_contract_version = 5;

    let compatible =
        CompositionCandidate::new("engine.render", "provider.v2", "provider.v2", 0, 0, 0)
            .with_capability_metadata([
                CompositionCapabilityMetadata::new("asset.format.read")
                    .with_version(2)
                    .with_contract("editor.asset.read", Some(3))
                    .with_tags(["feature.shadow"]),
                CompositionCapabilityMetadata::new("asset.preview.texture")
                    .with_version(4)
                    .with_contract("editor.asset.preview", Some(5))
                    .with_tags(["feature.shadow"]),
            ]);
    let wrong_preview_version =
        CompositionCandidate::new("engine.render", "provider.old", "provider.old", 1000, 0, 0)
            .with_capability_metadata([
                CompositionCapabilityMetadata::new("asset.format.read")
                    .with_version(2)
                    .with_contract("editor.asset.read", Some(3))
                    .with_tags(["feature.shadow"]),
                CompositionCapabilityMetadata::new("asset.preview.texture")
                    .with_version(3)
                    .with_contract("editor.asset.preview", Some(5))
                    .with_tags(["feature.shadow"]),
            ]);

    let plan = CompositionSolver::resolve_input(CompositionSolverInput {
        candidates: vec![wrong_preview_version, compatible],
        capability_matrix: CapabilityMatrix::new(vec![read, preview]),
    });
    assert_eq!(
        plan.selected("engine.render")
            .map(|selection| selection.candidate_id.as_str()),
        Some("provider.v2")
    );
}

#[test]
fn explanation_graph_reports_rejections_shadowing_and_score_breakdown() {
    let mut requirement = CompositionRequirement::from_spec(
        &CapabilityRequirement::required(RENDER)
            .with_required_tags(&[SHADOW])
            .with_preferred_tags(&[TIMELINE]),
        "explain-test",
    );
    requirement.min_capability_version = 2;
    let matrix = CapabilityMatrix::new(vec![requirement]);

    let selected = candidate("render.selected", 20)
        .with_capability_version(2)
        .with_tags(["feature.shadow", "feature.timeline"]);
    let shadowed = candidate("render.shadowed", 10)
        .with_capability_version(2)
        .with_tags(["feature.shadow"]);
    let rejected = candidate("render.rejected", 10_000)
        .with_capability_version(1)
        .with_tags(["feature.timeline"]);

    let plan = CompositionSolver::resolve_input(CompositionSolverInput {
        candidates: vec![rejected, shadowed, selected],
        capability_matrix: matrix,
    });
    let explanation = plan.explanation().gateway("engine.render").unwrap();
    let selected = explanation
        .candidates
        .iter()
        .find(|candidate| candidate.candidate_id == "render.selected")
        .unwrap();
    assert_eq!(
        selected.disposition,
        CompositionCandidateDisposition::Selected
    );
    assert_eq!(selected.score.backend_priority, 20);
    assert_eq!(selected.score.preferred_tag_bonus, PREFERRED_TAG_BONUS);
    assert_eq!(selected.score.total, 20_000 + 20 + PREFERRED_TAG_BONUS);

    let shadowed = explanation
        .candidates
        .iter()
        .find(|candidate| candidate.candidate_id == "render.shadowed")
        .unwrap();
    assert_eq!(
        shadowed.disposition,
        CompositionCandidateDisposition::Shadowed
    );
    assert_eq!(shadowed.rank, Some(2));
    assert_eq!(shadowed.outranked_by, vec!["render.selected".to_owned()]);
    assert!(shadowed.rejection_reasons.is_empty());

    let rejected = explanation
        .candidates
        .iter()
        .find(|candidate| candidate.candidate_id == "render.rejected")
        .unwrap();
    assert_eq!(
        rejected.disposition,
        CompositionCandidateDisposition::Rejected
    );
    assert!(rejected
        .rejection_reasons
        .iter()
        .any(|reason| { reason.kind == CompositionRejectionKind::CapabilityVersionBelowMinimum }));
    assert!(rejected
        .rejection_reasons
        .iter()
        .any(|reason| { reason.kind == CompositionRejectionKind::MissingRequiredTag }));
}

#[test]
fn preflight_rejection_is_preserved_by_shared_explanation_graph() {
    let candidate = candidate("render.format-mismatch", 1000).with_preflight_rejection(
        CompositionRejectionReason::new(CompositionRejectionKind::FormatMismatch)
            .with_expected(".ytd")
            .with_actual(".ydd"),
    );
    let plan = CompositionSolver::resolve([candidate]);
    assert!(plan.selected("engine.render").is_none());
    let explanation = plan.explanation().gateway("engine.render").unwrap();
    assert_eq!(
        explanation.candidates[0].disposition,
        CompositionCandidateDisposition::Rejected
    );
    assert_eq!(
        explanation.candidates[0].rejection_reasons[0].kind,
        CompositionRejectionKind::FormatMismatch
    );
}

#[test]
fn parser_extracts_versioned_contract() {
    assert_eq!(
        parse_versioned_contract_id("newengine.render-provider/v12"),
        Some(("newengine.render-provider".to_owned(), 12))
    );
}
