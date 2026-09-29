use super::*;

#[derive(Debug, Clone, Default)]
pub struct CompositionSolverInput {
    pub candidates: Vec<CompositionCandidate>,
    pub capability_matrix: CapabilityMatrix,
}

pub struct CompositionSolver;

impl CompositionSolver {
    #[inline]
    pub const fn score(origin_bias: i64, backend_priority: i32, preference_bonus: i64) -> i64 {
        origin_bias + backend_priority as i64 + preference_bonus
    }

    pub fn resolve<I>(candidates: I) -> CompositionPlan
    where
        I: IntoIterator<Item = CompositionCandidate>,
    {
        Self::resolve_input(CompositionSolverInput {
            candidates: candidates.into_iter().collect(),
            capability_matrix: CapabilityMatrix::default(),
        })
    }

    pub fn resolve_input(input: CompositionSolverInput) -> CompositionPlan {
        let CompositionSolverInput {
            candidates,
            capability_matrix,
        } = input;
        let mut by_gateway: BTreeMap<String, Vec<CompositionCandidate>> = BTreeMap::new();
        for candidate in candidates {
            if candidate.gateway_id.trim().is_empty() || candidate.candidate_id.trim().is_empty() {
                continue;
            }
            by_gateway
                .entry(candidate.gateway_id.clone())
                .or_default()
                .push(candidate);
        }
        for requirement in capability_matrix.capability_requirements() {
            by_gateway
                .entry(requirement.gateway_id.clone())
                .or_default();
        }

        let mut gateways = Vec::with_capacity(by_gateway.len());
        let mut explanations = Vec::with_capacity(by_gateway.len());
        let mut unsatisfied = Vec::new();
        for (gateway_id, candidates) in by_gateway {
            let requirement = capability_matrix.requirement(&gateway_id);
            let typed_requirements =
                capability_matrix.capability_requirements_for_gateway(&gateway_id);
            // One authoritative evaluation pass per candidate. `explain_candidate`
            // already computes preflight/composition/typed requirement rejection and
            // the immutable score breakdown, so ranking must not repeat those scans.
            let mut candidate_explanations = Vec::with_capacity(candidates.len());
            let mut ranked = Vec::with_capacity(candidates.len());
            for candidate in candidates {
                let explanation = explain_candidate(
                    &capability_matrix,
                    requirement,
                    typed_requirements,
                    &candidate,
                );
                if explanation.rejection_reasons.is_empty() {
                    ranked.push((candidate, explanation.score.total));
                }
                candidate_explanations.push(explanation);
            }

            if let Some(req) = requirement {
                let has_non_fallback = ranked
                    .iter()
                    .any(|(candidate, _)| !is_fallback_candidate(req, candidate));
                if has_non_fallback {
                    let suppressed = ranked
                        .iter()
                        .filter(|(candidate, _)| is_fallback_candidate(req, candidate))
                        .map(|(candidate, _)| candidate.candidate_id.clone())
                        .collect::<BTreeSet<_>>();
                    ranked.retain(|(candidate, _)| !suppressed.contains(&candidate.candidate_id));
                    for explanation in &mut candidate_explanations {
                        if suppressed.contains(&explanation.candidate_id) {
                            explanation.disposition = CompositionCandidateDisposition::Rejected;
                            push_reason_unique(
                                &mut explanation.rejection_reasons,
                                CompositionRejectionReason::new(
                                    CompositionRejectionKind::FallbackSuppressed,
                                )
                                .with_expected("non-fallback candidate available"),
                            );
                        }
                    }
                }
            }

            ranked.sort_by(|(a, a_score), (b, b_score)| {
                b_score
                    .cmp(a_score)
                    .then_with(|| b.backend_priority.cmp(&a.backend_priority))
                    .then_with(|| b.origin_bias.cmp(&a.origin_bias))
                    .then_with(|| a.candidate_id.cmp(&b.candidate_id))
                    .then_with(|| a.provider_owner_id.cmp(&b.provider_owner_id))
            });

            let max = requirement
                .map(|req| usize::from(req.max_cardinality.max(1)))
                .unwrap_or(1);
            let selected_count = ranked.len().min(max);
            let selected = ranked
                .iter()
                .take(selected_count)
                .map(|(candidate, score)| selection_from_candidate(requirement, candidate, *score))
                .collect::<Vec<_>>();
            let shadowed = ranked
                .iter()
                .skip(selected_count)
                .map(|(candidate, score)| selection_from_candidate(requirement, candidate, *score))
                .collect::<Vec<_>>();

            let selected_ids = selected
                .iter()
                .map(|entry| entry.candidate_id.clone())
                .collect::<Vec<_>>();
            let rank_by_id = ranked
                .iter()
                .enumerate()
                .map(|(index, (candidate, _))| (candidate.candidate_id.as_str(), index + 1))
                .collect::<BTreeMap<_, _>>();
            let selected_id_set = selected_ids
                .iter()
                .map(String::as_str)
                .collect::<BTreeSet<_>>();
            let shadowed_ids = shadowed
                .iter()
                .map(|entry| entry.candidate_id.as_str())
                .collect::<BTreeSet<_>>();
            for explanation in &mut candidate_explanations {
                explanation.rank = rank_by_id.get(explanation.candidate_id.as_str()).copied();
                if selected_id_set.contains(explanation.candidate_id.as_str()) {
                    explanation.disposition = CompositionCandidateDisposition::Selected;
                } else if shadowed_ids.contains(explanation.candidate_id.as_str()) {
                    explanation.disposition = CompositionCandidateDisposition::Shadowed;
                    explanation.outranked_by = selected_ids.clone();
                }
            }
            candidate_explanations.sort_by(|a, b| {
                disposition_rank(a.disposition)
                    .cmp(&disposition_rank(b.disposition))
                    .then_with(|| b.score.total.cmp(&a.score.total))
                    .then_with(|| a.candidate_id.cmp(&b.candidate_id))
            });

            if let Some(req) = requirement {
                if selected.len() < usize::from(req.min_cardinality) {
                    unsatisfied.push(UnsatisfiedCapabilityRequirement {
                        gateway_id: gateway_id.clone(),
                        level: req.level,
                        required_min: req.min_cardinality,
                        resolved: selected.len().min(usize::from(u16::MAX)) as u16,
                        declared_by: req.declared_by.clone(),
                    });
                }
            }

            explanations.push(GatewayCompositionExplanation {
                gateway_id: gateway_id.clone(),
                requirements: typed_requirements.to_vec(),
                candidates: candidate_explanations,
            });
            gateways.push(GatewayCompositionPlan {
                gateway_id,
                selected,
                shadowed,
            });
        }

        CompositionPlan {
            gateways,
            unsatisfied,
            explanation: CompositionExplanationGraph {
                gateways: explanations,
                contract_resolutions: Vec::new(),
            },
        }
    }
}

fn candidate_score_breakdown(
    matrix: &CapabilityMatrix,
    requirement: Option<&CompositionRequirement>,
    candidate: &CompositionCandidate,
) -> CompositionScoreBreakdown {
    let mut preferred_tags = matrix
        .preferred_tags()
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    if let Some(requirement) = requirement {
        preferred_tags.extend(requirement.preferred_tags.iter().map(String::as_str));
    }
    let candidate_tags = candidate_all_tags(candidate);
    let preferred_tag_matches = preferred_tags
        .iter()
        .filter(|tag| candidate_tags.contains(**tag))
        .map(|tag| (*tag).to_owned())
        .collect::<Vec<_>>();
    let preferred_tag_bonus = preferred_tag_matches.len() as i64 * PREFERRED_TAG_BONUS;
    let total = CompositionSolver::score(
        candidate.origin_bias,
        candidate.backend_priority,
        candidate.preference_bonus + preferred_tag_bonus,
    );
    CompositionScoreBreakdown {
        origin_bias: candidate.origin_bias,
        backend_priority: candidate.backend_priority,
        base_preference_bonus: candidate.preference_bonus,
        preferred_tag_matches,
        preferred_tag_bonus,
        total,
    }
}

fn candidate_all_tags(candidate: &CompositionCandidate) -> BTreeSet<&str> {
    candidate
        .tags
        .iter()
        .map(String::as_str)
        .chain(
            candidate
                .capability_metadata
                .iter()
                .flat_map(|capability| capability.tags.iter().map(String::as_str)),
        )
        .collect()
}

fn candidate_tags_for_requirement<'a>(
    candidate: &'a CompositionCandidate,
    capability: Option<&'a CompositionCapabilityMetadata>,
) -> BTreeSet<&'a str> {
    candidate
        .tags
        .iter()
        .map(String::as_str)
        .chain(
            capability
                .into_iter()
                .flat_map(|capability| capability.tags.iter().map(String::as_str)),
        )
        .collect()
}

fn disposition_rank(disposition: CompositionCandidateDisposition) -> u8 {
    match disposition {
        CompositionCandidateDisposition::Selected => 0,
        CompositionCandidateDisposition::Shadowed => 1,
        CompositionCandidateDisposition::Rejected => 2,
    }
}

fn push_reason_unique(
    reasons: &mut Vec<CompositionRejectionReason>,
    reason: CompositionRejectionReason,
) {
    if !reasons.contains(&reason) {
        reasons.push(reason);
    }
}

fn explain_candidate(
    matrix: &CapabilityMatrix,
    requirement: Option<&CompositionRequirement>,
    typed_requirements: &[CompositionRequirement],
    candidate: &CompositionCandidate,
) -> CompositionCandidateExplanation {
    let mut rejection_reasons = candidate.preflight_rejections.clone();
    let tags = candidate_all_tags(candidate);
    for conflict in matrix.conflict_tags() {
        if tags.contains(conflict.as_str()) {
            push_reason_unique(
                &mut rejection_reasons,
                CompositionRejectionReason::new(CompositionRejectionKind::CompositionForbiddenTag)
                    .with_expected(format!("tag '{}' must be absent", conflict))
                    .with_actual(conflict.clone()),
            );
        }
    }

    let requirement_evaluations = typed_requirements
        .iter()
        .map(|req| {
            let reasons = candidate_requirement_rejections(req, candidate);
            for reason in &reasons {
                push_reason_unique(&mut rejection_reasons, reason.clone());
            }
            CompositionRequirementEvaluation {
                capability_id: req.capability_id.clone(),
                accepted: reasons.is_empty(),
                rejection_reasons: reasons,
            }
        })
        .collect::<Vec<_>>();

    CompositionCandidateExplanation {
        gateway_id: candidate.gateway_id.clone(),
        candidate_id: candidate.candidate_id.clone(),
        provider_owner_id: candidate.provider_owner_id.clone(),
        disposition: if rejection_reasons.is_empty() {
            CompositionCandidateDisposition::Shadowed
        } else {
            CompositionCandidateDisposition::Rejected
        },
        rank: None,
        outranked_by: Vec::new(),
        fallback: requirement.is_some_and(|req| is_fallback_candidate(req, candidate)),
        score: candidate_score_breakdown(matrix, requirement, candidate),
        requirement_evaluations,
        rejection_reasons,
    }
}

fn candidate_requirement_rejections(
    requirement: &CompositionRequirement,
    candidate: &CompositionCandidate,
) -> Vec<CompositionRejectionReason> {
    let mut reasons = Vec::new();
    let typed_capability = candidate
        .capability_metadata
        .iter()
        .find(|capability| capability.id == requirement.capability_id);

    let capability_present = if !candidate.capability_metadata.is_empty() {
        typed_capability.is_some()
    } else if candidate.capability_id.is_some() || !candidate.capability_ids.is_empty() {
        candidate
            .capability_id
            .as_deref()
            .is_some_and(|capability| capability == requirement.capability_id)
            || candidate
                .capability_ids
                .iter()
                .any(|capability| capability == &requirement.capability_id)
    } else {
        true
    };
    if !capability_present {
        reasons.push(
            CompositionRejectionReason::new(CompositionRejectionKind::MissingCapability)
                .for_capability(requirement.capability_id.clone())
                .with_expected(requirement.capability_id.clone()),
        );
        return reasons;
    }

    if requirement.min_capability_version > 0 || requirement.max_capability_version.is_some() {
        let version = typed_capability
            .and_then(|capability| capability.version)
            .or(candidate.capability_version);
        match version {
            None => reasons.push(
                CompositionRejectionReason::new(CompositionRejectionKind::MissingCapabilityVersion)
                    .for_capability(requirement.capability_id.clone())
                    .with_expected(format!(
                        "{}..{}",
                        requirement.min_capability_version,
                        requirement
                            .max_capability_version
                            .map(|value| value.to_string())
                            .unwrap_or_else(|| "*".to_owned())
                    )),
            ),
            Some(version) if version < requirement.min_capability_version => reasons.push(
                CompositionRejectionReason::new(
                    CompositionRejectionKind::CapabilityVersionBelowMinimum,
                )
                .for_capability(requirement.capability_id.clone())
                .with_expected(format!(">={}", requirement.min_capability_version))
                .with_actual(version.to_string()),
            ),
            Some(version)
                if requirement
                    .max_capability_version
                    .is_some_and(|max_version| version > max_version) =>
            {
                reasons.push(
                    CompositionRejectionReason::new(
                        CompositionRejectionKind::CapabilityVersionAboveMaximum,
                    )
                    .for_capability(requirement.capability_id.clone())
                    .with_expected(format!("<={}", requirement.max_capability_version.unwrap()))
                    .with_actual(version.to_string()),
                );
            }
            _ => {}
        }
    }

    if let Some(required_contract) = requirement.contract_id.as_deref() {
        let contract_id = typed_capability
            .and_then(|capability| capability.contract_id.as_deref())
            .or(candidate.contract_id.as_deref());
        if contract_id != Some(required_contract) {
            reasons.push(
                CompositionRejectionReason::new(CompositionRejectionKind::ContractMismatch)
                    .for_capability(requirement.capability_id.clone())
                    .with_expected(required_contract.to_owned())
                    .with_actual(contract_id.unwrap_or("<none>").to_owned()),
            );
        } else {
            let contract_version = typed_capability
                .and_then(|capability| capability.contract_version)
                .or(candidate.contract_version);
            match contract_version {
                Some(version) if version < requirement.min_contract_version => reasons.push(
                    CompositionRejectionReason::new(
                        CompositionRejectionKind::ContractVersionBelowMinimum,
                    )
                    .for_capability(requirement.capability_id.clone())
                    .with_expected(format!(">={}", requirement.min_contract_version))
                    .with_actual(version.to_string()),
                ),
                Some(version)
                    if requirement
                        .max_contract_version
                        .is_some_and(|max_version| version > max_version) =>
                {
                    reasons.push(
                        CompositionRejectionReason::new(
                            CompositionRejectionKind::ContractVersionAboveMaximum,
                        )
                        .for_capability(requirement.capability_id.clone())
                        .with_expected(format!("<={}", requirement.max_contract_version.unwrap()))
                        .with_actual(version.to_string()),
                    );
                }
                None if requirement.min_contract_version > 0
                    || requirement.max_contract_version.is_some() =>
                {
                    reasons.push(
                        CompositionRejectionReason::new(
                            CompositionRejectionKind::MissingContractVersion,
                        )
                        .for_capability(requirement.capability_id.clone())
                        .with_expected(format!(
                            "{}..{}",
                            requirement.min_contract_version,
                            requirement
                                .max_contract_version
                                .map(|value| value.to_string())
                                .unwrap_or_else(|| "*".to_owned())
                        )),
                    );
                }
                _ => {}
            }
        }
    }

    let tags = candidate_tags_for_requirement(candidate, typed_capability);
    for required in &requirement.required_tags {
        if !tags.contains(required.as_str()) {
            reasons.push(
                CompositionRejectionReason::new(CompositionRejectionKind::MissingRequiredTag)
                    .for_capability(requirement.capability_id.clone())
                    .with_expected(required.clone()),
            );
        }
    }
    for conflict in &requirement.conflict_tags {
        if tags.contains(conflict.as_str()) {
            reasons.push(
                CompositionRejectionReason::new(CompositionRejectionKind::ForbiddenTag)
                    .for_capability(requirement.capability_id.clone())
                    .with_expected(format!("tag '{}' must be absent", conflict))
                    .with_actual(conflict.clone()),
            );
        }
    }
    reasons.sort();
    reasons.dedup();
    reasons
}

fn is_fallback_candidate(
    requirement: &CompositionRequirement,
    candidate: &CompositionCandidate,
) -> bool {
    requirement.fallback_provider_ids.iter().any(|fallback| {
        fallback == &candidate.candidate_id || fallback == &candidate.provider_owner_id
    })
}

fn selection_from_candidate(
    requirement: Option<&CompositionRequirement>,
    candidate: &CompositionCandidate,
    score: i64,
) -> CompositionSelection {
    CompositionSelection {
        gateway_id: candidate.gateway_id.clone(),
        candidate_id: candidate.candidate_id.clone(),
        provider_owner_id: candidate.provider_owner_id.clone(),
        backend_priority: candidate.backend_priority,
        origin_bias: candidate.origin_bias,
        score,
        fallback: requirement.is_some_and(|req| is_fallback_candidate(req, candidate)),
    }
}

/// Parses ABI ids of the form `contract.name/v3` into (`contract.name`, 3).
pub fn parse_versioned_contract_id(value: &str) -> Option<(String, u32)> {
    let value = value.trim();
    let (contract, raw_version) = value.rsplit_once("/v")?;
    if contract.trim().is_empty() {
        return None;
    }
    let version = raw_version.parse::<u32>().ok()?;
    Some((contract.to_owned(), version))
}
