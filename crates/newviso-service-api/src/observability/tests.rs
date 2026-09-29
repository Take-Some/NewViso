    use super::*;
    use crate::{
        CapabilityMatrix, CompositionCandidate, CompositionSolver, CompositionSolverInput,
    };

    fn plan(priority: i32) -> CompositionPlan {
        CompositionSolver::resolve_input(CompositionSolverInput {
            candidates: vec![CompositionCandidate::new(
                "engine.demo",
                "provider.demo",
                "provider.demo",
                priority,
                20_000,
                0,
            )
            .with_capability("demo.backend")],
            capability_matrix: CapabilityMatrix::default(),
        })
    }

    #[test]
    fn snapshot_json_round_trip_keeps_stable_schema_and_identity() {
        let snapshot = CompositionSnapshotV1::from_plan(
            7,
            11,
            22,
            CompositionSnapshotProvenanceV1::frozen("host.frozen_composition_plan"),
            &plan(10),
        );
        let json = snapshot.to_json().expect("serialize snapshot");
        assert!(json.contains("\"schema\":\"composition.snapshot_v1\""));
        let decoded = CompositionSnapshotV1::from_json(&json).expect("parse snapshot");
        assert_eq!(decoded, snapshot);
        assert_eq!(decoded.instance_id, 7);
        assert_eq!(decoded.composition_epoch, 11);
        assert_eq!(decoded.topology_generation, 22);
    }

    #[test]
    fn diff_reports_candidate_score_change_between_epochs_deterministically() {
        let before = CompositionSnapshotV1::from_plan(
            7,
            10,
            20,
            CompositionSnapshotProvenanceV1::live("runtime.gateway_registry"),
            &plan(10),
        );
        let after = CompositionSnapshotV1::from_plan(
            7,
            11,
            22,
            CompositionSnapshotProvenanceV1::live("runtime.gateway_registry"),
            &plan(25),
        );
        let diff = CompositionDiffV1::between(&before, &after).expect("diff");
        assert!(diff.same_instance);
        assert_eq!(diff.from_epoch, 10);
        assert_eq!(diff.to_epoch, 11);
        assert_eq!(diff.gateway_changes.len(), 1);
        let change = &diff.gateway_changes[0].candidate_changes[0];
        assert_eq!(change.candidate_id, "provider.demo");
        assert_eq!(
            change.before_score.as_ref().map(|score| score.total),
            Some(20_010)
        );
        assert_eq!(
            change.after_score.as_ref().map(|score| score.total),
            Some(20_025)
        );
        assert_ne!(change.before_score, change.after_score);
        let json = diff.to_json().expect("serialize diff");
        assert_eq!(
            CompositionDiffV1::from_json(&json).expect("parse diff"),
            diff
        );
    }

    #[test]
    fn snapshot_v1_rejects_unstable_or_incoherent_epoch_metadata() {
        let mut snapshot = CompositionSnapshotV1::from_plan(
            1,
            10,
            20,
            CompositionSnapshotProvenanceV1::live("runtime.gateway_registry"),
            &plan(10),
        );
        snapshot.topology_generation = 21;
        assert!(snapshot
            .validate_schema()
            .unwrap_err()
            .contains("stable even"));
        snapshot.topology_generation = 20;
        snapshot.composition_epoch = 11;
        assert!(snapshot
            .validate_schema()
            .unwrap_err()
            .contains("epoch/generation mismatch"));
    }

    #[test]
    fn identical_semantic_snapshots_have_empty_diff_even_when_epoch_advances() {
        let plan = plan(10);
        let before = CompositionSnapshotV1::from_plan(
            7,
            10,
            20,
            CompositionSnapshotProvenanceV1::live("runtime.gateway_registry"),
            &plan,
        );
        let after = CompositionSnapshotV1::from_plan(
            7,
            11,
            22,
            CompositionSnapshotProvenanceV1::live("runtime.gateway_registry"),
            &plan,
        );
        assert!(CompositionDiffV1::between(&before, &after)
            .expect("diff")
            .is_empty());
    }
