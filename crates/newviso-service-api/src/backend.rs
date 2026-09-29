use std::collections::BTreeMap;

use serde::Serialize;

use crate::{system_tag, CapabilityId};

/// Common declaration for a backend service family.
///
/// This intentionally does not describe domain packets. Render, physics, input,
/// UI and future domains still own their DTOs and typed adapters; this spec only
/// tells the host which service id and backend capability must co-exist on the
/// provider plugin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackendServiceSpec {
    /// Human-readable domain label used in diagnostics.
    pub domain: &'static str,
    /// Stable engine-facing gateway id consumers call, e.g. `engine.render`.
    pub engine_gateway_id: &'static str,
    /// First-party/default provider service id, e.g. `render.api`.
    pub provider_service_id: &'static str,
    /// Logical backend capability id declared by provider plugins.
    pub backend_capability_id: &'static str,
}

impl BackendServiceSpec {
    #[inline]
    pub const fn new(
        domain: &'static str,
        engine_gateway_id: &'static str,
        provider_service_id: &'static str,
        backend_capability_id: &'static str,
    ) -> Self {
        Self {
            domain,
            engine_gateway_id,
            provider_service_id,
            backend_capability_id,
        }
    }

    /// Returns the logical composition capability bound to this backend contract.
    /// Product profiles use this instead of repeating gateway/service-kind strings.
    #[inline]
    pub const fn capability(self) -> CapabilityId {
        CapabilityId::new(
            self.backend_capability_id,
            self.engine_gateway_id,
            self.domain,
        )
    }
}

/// Typed provider route metadata serialized into backend capability JSON.
#[derive(Debug, Clone, Serialize)]
pub struct BackendRouteDescriptor {
    pub service_kind: &'static str,
    pub engine_gateway: &'static str,
    pub contract: &'static str,
    pub backend_priority: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_abi: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_route: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<&'static str>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub features: Vec<&'static str>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub system_tags: Vec<&'static str>,
    #[serde(flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<&'static str, serde_json::Value>,
}

impl BackendRouteDescriptor {
    #[inline]
    pub fn new(spec: BackendServiceSpec) -> Self {
        let service_kind = spec.domain;
        debug_assert!(
            !service_kind.trim().is_empty(),
            "BackendServiceSpec domain must not be empty",
        );
        Self {
            service_kind,
            engine_gateway: spec.engine_gateway_id,
            contract: spec.provider_service_id,
            backend_priority: 0,
            provider_abi: None,
            provider_route: None,
            backend: None,
            mode: None,
            features: Vec::new(),
            system_tags: Vec::new(),
            metadata: BTreeMap::new(),
        }
    }

    #[inline]
    pub fn contract(mut self, contract: &'static str) -> Self {
        self.contract = contract;
        self
    }

    #[inline]
    pub fn engine_gateway(mut self, engine_gateway: &'static str) -> Self {
        self.engine_gateway = engine_gateway;
        self
    }

    #[inline]
    pub fn provider_abi(mut self, provider_abi: &'static str) -> Self {
        self.provider_abi = Some(provider_abi);
        self
    }

    #[inline]
    pub fn provider_route(mut self, provider_route: &'static str) -> Self {
        self.provider_route = Some(provider_route);
        self.system_tags
            .push(system_tag::PROVIDER_IMPLEMENTATION_ROUTE);
        self
    }

    pub fn provider_implementation_route(mut self) -> Self {
        self.system_tags
            .push(system_tag::PROVIDER_IMPLEMENTATION_ROUTE);
        self
    }

    #[inline]
    pub fn priority(mut self, backend_priority: i32) -> Self {
        self.backend_priority = backend_priority;
        self
    }

    #[inline]
    pub fn backend(mut self, backend: &'static str) -> Self {
        self.backend = Some(backend);
        self
    }

    #[inline]
    pub fn mode(mut self, mode: &'static str) -> Self {
        self.mode = Some(mode);
        self
    }

    #[inline]
    pub fn feature(mut self, feature: &'static str) -> Self {
        self.features.push(feature);
        self
    }

    #[inline]
    pub fn features(mut self, features: impl IntoIterator<Item = &'static str>) -> Self {
        self.features.extend(features);
        self
    }

    #[inline]
    pub fn system_tag(mut self, tag: &'static str) -> Self {
        self.system_tags.push(tag);
        self
    }

    #[inline]
    pub fn system_tags(mut self, tags: impl IntoIterator<Item = &'static str>) -> Self {
        self.system_tags.extend(tags);
        self
    }

    #[inline]
    pub fn metadata_json(mut self, key: &'static str, value: serde_json::Value) -> Self {
        self.metadata.insert(key, value);
        self
    }

    #[inline]
    pub fn to_json_string(&self) -> String {
        serde_json::to_string(self).expect("BackendRouteDescriptor must serialize to JSON")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_SPEC: BackendServiceSpec =
        BackendServiceSpec::new("render", "engine.render", "render.api", "render.backend");

    #[test]
    fn backend_spec_exposes_logical_composition_capability() {
        let capability = TEST_SPEC.capability();
        assert_eq!(capability.as_str(), "render.backend");
        assert_eq!(capability.gateway_id(), "engine.render");
        assert_eq!(capability.service_kind(), "render");
    }

    #[test]
    fn backend_route_provider_abi_is_optional_and_serialized_when_present() {
        let legacy = BackendRouteDescriptor::new(TEST_SPEC).to_json_string();
        assert!(!legacy.contains("provider_abi"));

        let versioned = BackendRouteDescriptor::new(TEST_SPEC)
            .provider_abi("newengine.render-provider/v1")
            .to_json_string();
        let value: serde_json::Value = serde_json::from_str(&versioned).unwrap();
        assert_eq!(
            value
                .get("provider_abi")
                .and_then(serde_json::Value::as_str),
            Some("newengine.render-provider/v1")
        );
    }
}
