use super::*;
use abi_stable::std_types::ROption;

#[test]
fn legacy_route_json_normalizes_once_into_typed_v2() {
    let json = serde_json::json!({
        "service_kind": "render",
        "engine_gateway": "engine.render",
        "contract": "engine.render.provider",
        "provider_route": "engine.render.vulkan",
        "provider_abi": "newengine.render-provider/v1",
        "backend_priority": 42,
        "backend": "vulkan",
        "features": ["ray-query", "mesh-shader"],
        "tags": ["provider.backend"]
    })
    .to_string();

    let legacy = CapabilityDesc::new(
        "engine.render.vulkan.backend",
        CapabilityRole::Provides,
        CapabilityKind::Other,
        3,
    )
    .with_json(json.clone());
    let typed = legacy.to_v2_compat();

    assert_eq!(typed.version, 3);
    assert!(typed.has_tag("provider.backend"));
    assert!(typed.has_tag("backend.vulkan"));
    assert!(typed.has_tag("feature.ray.query"));
    assert_eq!(typed.extension_json.as_str(), json);

    let ROption::RSome(route) = typed.route else {
        panic!("legacy gateway metadata must normalize to a typed route");
    };
    assert_eq!(route.service_kind.as_str(), "render");
    assert_eq!(route.engine_gateway.as_str(), "engine.render");
    assert_eq!(route.provider_service_id.as_str(), "engine.render.provider");
    assert_eq!(route.backend_priority, 42);
    assert_eq!(
        route.provider_abi.into_option().as_deref(),
        Some("newengine.render-provider/v1")
    );
}

#[test]
fn direct_v2_route_does_not_require_json_semantics() {
    let route = BackendRouteDescriptorV2 {
        service_kind: "render".into(),
        engine_gateway: "engine.render".into(),
        provider_service_id: "engine.render.provider".into(),
        provider_abi: ROption::RSome("newengine.render-provider/v2".into()),
        provider_route: ROption::RSome("engine.render.vulkan".into()),
        backend_priority: 100,
        backend: ROption::RSome("vulkan".into()),
        mode: ROption::RNone,
        features: vec!["mesh-shader".into()].into(),
    };
    let typed = CapabilityDescV2::new(
        "engine.render.vulkan.backend",
        CapabilityRole::Provides,
        CapabilityKind::Other,
        7,
    )
    .with_contract(ContractRefV2::new("engine.render.provider", 2))
    .with_tag("provider.backend")
    .with_route(route);

    assert!(typed.extension_json.is_empty());
    assert_eq!(typed.version, 7);
    assert!(typed.has_tag("provider.backend"));
    let ROption::RSome(contract) = typed.contract else {
        panic!("typed contract is required");
    };
    assert_eq!(contract.id.as_str(), "engine.render.provider");
    assert_eq!(contract.version.into_option(), Some(2));
}

#[test]
fn malformed_legacy_extension_does_not_create_typed_route() {
    let legacy = CapabilityDesc::new(
        "broken.metadata",
        CapabilityRole::Provides,
        CapabilityKind::Other,
        1,
    )
    .with_json("{not-json");
    let typed = legacy.to_v2_compat();
    assert!(matches!(typed.route, ROption::RNone));
    assert_eq!(typed.extension_json.as_str(), "{not-json");
}
