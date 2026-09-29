    use super::*;

    const RENDER: CapabilityId = CapabilityId::new("render.backend", "engine.render", "render");
    const AUDIO: CapabilityId = CapabilityId::new("audio.backend", "engine.audio", "audio");
    const TIMELINE: SystemTag = SystemTag::new("feature.timeline");

    const REQUIREMENTS: &[CapabilityRequirement] = &[
        CapabilityRequirement::required(RENDER)
            .with_contract("newengine.render-provider", 1, Some(2))
            .with_preferred_tags(&[TIMELINE]),
        CapabilityRequirement::preferred(AUDIO)
            .with_fallback(FallbackPolicy::Providers(&["engine.audio.null"])),
    ];

    #[test]
    fn composition_v2_is_capability_first() {
        let spec = EngineCompositionSpec::new("test.composition", REQUIREMENTS);
        assert_eq!(spec.schema_version, EngineCompositionSpec::SCHEMA_VERSION);
        assert_eq!(spec.requirements[0].capability.as_str(), "render.backend");
        assert_eq!(
            spec.requirements[0].capability.gateway_id(),
            "engine.render"
        );
        assert!(spec.requirements[0].strength.is_required());
        assert_eq!(
            spec.requirements[0]
                .contract
                .expect("render contract")
                .min_version,
            1
        );
        assert_eq!(
            spec.requirements[1].strength,
            RequirementStrength::Preferred
        );
    }

    #[test]
    fn many_cardinality_composes_with_strength() {
        assert_eq!(Cardinality::Many.min(RequirementStrength::Required), 1);
        assert_eq!(Cardinality::Many.min(RequirementStrength::Optional), 0);
        assert_eq!(Cardinality::Many.max(), u16::MAX);
    }
    #[test]
    fn runtime_unit_requirement_preserves_many_cardinality() {
        let requirement = RuntimeUnitRequirementSpec::required("render.feature")
            .with_cardinality(Cardinality::Many);
        assert_eq!(requirement.capability, "render.feature");
        assert_eq!(requirement.strength, RequirementStrength::Required);
        assert_eq!(requirement.cardinality.min(requirement.strength), 1);
        assert_eq!(requirement.cardinality.max(), u16::MAX);
    }
