/// Startup contract for a runtime service.
///
/// Domain crates may expose constants of this type; startup validation can then
/// walk specs instead of hard-coding a resolver per backend family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeServiceContractSpec {
    pub service_id: &'static str,
    pub expected_contract: &'static str,
    pub required_methods: &'static [&'static str],
}

impl RuntimeServiceContractSpec {
    #[inline]
    pub const fn new(
        service_id: &'static str,
        expected_contract: &'static str,
        required_methods: &'static [&'static str],
    ) -> Self {
        Self {
            service_id,
            expected_contract,
            required_methods,
        }
    }
}

/// Declarative startup policy for a runtime service gateway or direct host service.
///
/// This is intentionally data-only. The engine startup validator walks a catalog
/// of these specs; it must not branch on individual domains. Missing providers
/// degrade by default and become fatal only when `required_env` is set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeServiceRequirementSpec {
    pub contract: RuntimeServiceContractSpec,
    pub required_capability_id: Option<&'static str>,
    pub required_env: Option<&'static str>,
}

impl RuntimeServiceRequirementSpec {
    #[inline]
    pub const fn new(
        contract: RuntimeServiceContractSpec,
        required_capability_id: Option<&'static str>,
        required_env: Option<&'static str>,
    ) -> Self {
        Self {
            contract,
            required_capability_id,
            required_env,
        }
    }
}
