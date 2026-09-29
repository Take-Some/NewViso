use super::*;

#[test]
fn child_domains_parse_with_canonical_gateways() {
    let cases = [
        (
            "assets.vfs",
            EngineServiceKind::AssetVfs,
            "engine.assets.vfs",
            Some(EngineServiceKind::Assets),
        ),
        (
            "assets.types",
            EngineServiceKind::AssetTypes,
            "engine.assets.types",
            Some(EngineServiceKind::Assets),
        ),
        (
            "assets.inspect",
            EngineServiceKind::AssetInspect,
            "engine.assets.inspect",
            Some(EngineServiceKind::Assets),
        ),
        (
            "assets.edit",
            EngineServiceKind::AssetEdit,
            "engine.assets.edit",
            Some(EngineServiceKind::Assets),
        ),
        (
            "assets.packages",
            EngineServiceKind::AssetPackages,
            "engine.assets.packages",
            Some(EngineServiceKind::Assets),
        ),
        (
            "assets.listfiles",
            EngineServiceKind::AssetListFiles,
            "engine.assets.listfiles",
            Some(EngineServiceKind::Assets),
        ),
        (
            "assets.maps",
            EngineServiceKind::AssetMaps,
            "engine.assets.maps",
            Some(EngineServiceKind::Assets),
        ),
        (
            "assets.validation",
            EngineServiceKind::AssetValidation,
            "engine.assets.validation",
            Some(EngineServiceKind::Assets),
        ),
        (
            "assets.ui",
            EngineServiceKind::AssetUi,
            "engine.assets.ui",
            Some(EngineServiceKind::Assets),
        ),
        (
            "assets.materials",
            EngineServiceKind::Materials,
            "engine.assets.materials",
            Some(EngineServiceKind::Assets),
        ),
        (
            "assets.definitions",
            EngineServiceKind::Definitions,
            "engine.assets.definitions",
            Some(EngineServiceKind::Assets),
        ),
        (
            "assets.graph",
            EngineServiceKind::AssetGraph,
            "engine.assets.graph",
            Some(EngineServiceKind::Assets),
        ),
        ("time", EngineServiceKind::Time, "engine.time", None),
        ("schema", EngineServiceKind::Schema, "engine.schema", None),
        (
            "animation",
            EngineServiceKind::Animation,
            "engine.animation",
            None,
        ),
        (
            "navigation",
            EngineServiceKind::Navigation,
            "engine.navigation",
            None,
        ),
        ("ai", EngineServiceKind::Ai, "engine.ai", None),
        ("tags", EngineServiceKind::Tags, "engine.tags", None),
        ("tasks", EngineServiceKind::Tasks, "engine.tasks", None),
        (
            "threading",
            EngineServiceKind::Threading,
            "engine.threading",
            None,
        ),
        (
            "scripting",
            EngineServiceKind::Scripting,
            "engine.scripting",
            None,
        ),
        (
            "input.bindings",
            EngineServiceKind::InputBindings,
            "engine.input.bindings",
            Some(EngineServiceKind::Input),
        ),
        (
            "input.actions",
            EngineServiceKind::InputActions,
            "engine.input.actions",
            Some(EngineServiceKind::Input),
        ),
        (
            "input.contexts",
            EngineServiceKind::InputContexts,
            "engine.input.contexts",
            Some(EngineServiceKind::Input),
        ),
        (
            "render.effects",
            EngineServiceKind::RenderEffects,
            "engine.render.effects",
            Some(EngineServiceKind::Render),
        ),
        (
            "render.materials",
            EngineServiceKind::RenderMaterials,
            "engine.render.materials",
            Some(EngineServiceKind::Render),
        ),
        (
            "assets.models",
            EngineServiceKind::Model,
            "engine.assets.models",
            Some(EngineServiceKind::Assets),
        ),
        (
            "assets.models.skeletons",
            EngineServiceKind::ModelSkeletons,
            "engine.assets.models.skeletons",
            Some(EngineServiceKind::Model),
        ),
        (
            "assets.models.materials",
            EngineServiceKind::ModelMaterials,
            "engine.assets.models.materials",
            Some(EngineServiceKind::Model),
        ),
        (
            "assets.models.collisions",
            EngineServiceKind::ModelCollisions,
            "engine.assets.models.collisions",
            Some(EngineServiceKind::Model),
        ),
        (
            "physics.contacts",
            EngineServiceKind::PhysicsContacts,
            "engine.physics.contacts",
            Some(EngineServiceKind::Physics),
        ),
        (
            "physics.constraints",
            EngineServiceKind::PhysicsConstraints,
            "engine.physics.constraints",
            Some(EngineServiceKind::Physics),
        ),
        (
            "camera.modes",
            EngineServiceKind::CameraModes,
            "engine.camera.modes",
            Some(EngineServiceKind::Camera),
        ),
        (
            "camera.animations",
            EngineServiceKind::CameraAnimations,
            "engine.camera.animations",
            Some(EngineServiceKind::Camera),
        ),
        (
            "ui.text",
            EngineServiceKind::UiText,
            "engine.ui.text",
            Some(EngineServiceKind::Ui),
        ),
        (
            "ui.debug",
            EngineServiceKind::UiDebug,
            "engine.ui.debug",
            Some(EngineServiceKind::Ui),
        ),
    ];

    for (text, kind, gateway, parent) in cases {
        assert_eq!(EngineServiceKind::parse(text), Some(kind));
        assert_eq!(
            EngineServiceKind::parse_engine_gateway_id(gateway),
            Some(kind)
        );
        assert_eq!(kind.engine_gateway_id(), gateway);
        assert_eq!(kind.parent(), parent);
        assert!(kind.matches_engine_gateway_id(gateway));
    }
}

#[test]
fn parent_domain_does_not_match_child_gateway() {
    assert!(!EngineServiceKind::Assets.matches_engine_gateway_id("engine.assets.types"));
    assert!(!EngineServiceKind::Input.matches_engine_gateway_id("engine.input.bindings"));
    assert!(!EngineServiceKind::Render.matches_engine_gateway_id("engine.render.effects"));
    assert!(!EngineServiceKind::Physics.matches_engine_gateway_id("engine.physics.contacts"));
    assert!(!EngineServiceKind::Model.matches_engine_gateway_id("engine.assets.models.skeletons"));
    assert!(!EngineServiceKind::Camera.matches_engine_gateway_id("engine.camera.modes"));
    assert!(!EngineServiceKind::Ui.matches_engine_gateway_id("engine.ui.text"));
    assert!(!EngineServiceKind::Assets.matches_engine_gateway_id("engine.assets.ui"));
}

#[test]
fn engine_gateway_service_kind_is_dynamic_not_enum_bound() {
    assert_eq!(
        service_kind_from_engine_gateway_id("engine.assets.zzx").as_deref(),
        Some("assets.zzx")
    );
    assert!(engine_gateway_matches_service_kind(
        "engine.render.draw_lists",
        "render.draw_lists"
    ));
    assert!(EngineServiceKind::parse_engine_gateway_id("engine.assets.zzx").is_none());
}

#[test]
fn backend_route_descriptor_can_serialize_named_provider_route() {
    let json = BackendRouteDescriptor::new(BackendServiceSpec::new(
        "render",
        "engine.render",
        "render.api",
        "render.backend",
    ))
    .provider_route("engine.render.vulkan")
    .backend("vulkan")
    .to_json_string();

    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["service_kind"], "render");
    assert_eq!(value["engine_gateway"], "engine.render");
    assert_eq!(value["provider_route"], "engine.render.vulkan");
    assert_eq!(value["system_tags"][0], "provider.implementation_route");
    assert!(engine_gateway_is_direct_child_of_service_kind(
        "engine.render.vulkan",
        "render"
    ));
}

#[test]
fn backend_route_descriptor_serializes_registry_fields() {
    let json = BackendRouteDescriptor::new(BackendServiceSpec::new(
        "render",
        "engine.render",
        "render.api",
        "render.backend",
    ))
    .backend("native")
    .mode("graph-draw-list")
    .priority(100)
    .feature("draw-list")
    .metadata_json("shadows", serde_json::json!({ "pcss": true }))
    .to_json_string();

    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["service_kind"], "render");
    assert_eq!(value["engine_gateway"], "engine.render");
    assert_eq!(value["contract"], "render.api");
    assert_eq!(value["backend_priority"], 100);
    assert_eq!(value["backend"], "native");
    assert_eq!(value["mode"], "graph-draw-list");
    assert_eq!(value["features"][0], "draw-list");
    assert_eq!(value["shadows"]["pcss"], true);
}
