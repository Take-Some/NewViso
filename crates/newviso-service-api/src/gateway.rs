/// Reserved prefix for host-owned facade service gateways.
///
/// This is a namespace convention, not a concrete provider decision. Providers
/// declare a concrete gateway id in capability metadata; the host routes by the
/// descriptor table and never by domain-specific branches.
pub const ENGINE_SERVICE_GATEWAY_PREFIX: &str = "engine.";

// Canonical host-owned engine gateway vocabulary for cross-domain contracts.
// Domain API crates may re-export these under domain-specific names, but must
// not own independent string literals for the same gateway.
pub const ENGINE_ASSETS_GATEWAY_ID: &str = "engine.assets";
pub const ENGINE_ASSETS_VFS_GATEWAY_ID: &str = "engine.assets.vfs";
pub const ENGINE_ASSETS_TYPES_GATEWAY_ID: &str = "engine.assets.types";
pub const ENGINE_ASSETS_INSPECT_GATEWAY_ID: &str = "engine.assets.inspect";
pub const ENGINE_ASSETS_EDIT_GATEWAY_ID: &str = "engine.assets.edit";
pub const ENGINE_ASSETS_PACKAGES_GATEWAY_ID: &str = "engine.assets.packages";
pub const ENGINE_ASSETS_LISTFILES_GATEWAY_ID: &str = "engine.assets.listfiles";
pub const ENGINE_ASSETS_UID_GATEWAY_ID: &str = "engine.assets.uid";
pub const ENGINE_ASSETS_DEPENDENCIES_GATEWAY_ID: &str = "engine.assets.dependencies";
pub const ENGINE_ASSETS_IMPORT_QUEUE_GATEWAY_ID: &str = "engine.assets.import_queue";
pub const ENGINE_ASSETS_PACKAGE_WRITER_GATEWAY_ID: &str = "engine.assets.package_writer";
pub const ENGINE_ASSETS_MAPS_GATEWAY_ID: &str = "engine.assets.maps";
pub const ENGINE_ASSETS_VALIDATION_GATEWAY_ID: &str = "engine.assets.validation";
pub const ENGINE_ASSETS_UI_GATEWAY_ID: &str = "engine.assets.ui";
pub const ENGINE_ASSETS_MATERIALS_GATEWAY_ID: &str = "engine.assets.materials";
pub const ENGINE_ASSETS_DEFINITIONS_GATEWAY_ID: &str = "engine.assets.definitions";
pub const ENGINE_ASSETS_GRAPH_GATEWAY_ID: &str = "engine.assets.graph";
pub const ENGINE_ASSETS_MODELS_GATEWAY_ID: &str = "engine.assets.models";
pub const ENGINE_ASSETS_MODELS_SKELETONS_GATEWAY_ID: &str = "engine.assets.models.skeletons";
pub const ENGINE_ASSETS_MODELS_MATERIALS_GATEWAY_ID: &str = "engine.assets.models.materials";
pub const ENGINE_ASSETS_MODELS_COLLISIONS_GATEWAY_ID: &str = "engine.assets.models.collisions";

pub const ENGINE_RENDER_GATEWAY_ID: &str = "engine.render";
pub const ENGINE_RENDER_EFFECTS_GATEWAY_ID: &str = "engine.render.effects";
pub const ENGINE_RENDER_MATERIALS_GATEWAY_ID: &str = "engine.render.materials";
pub const ENGINE_PHYSICS_GATEWAY_ID: &str = "engine.physics";
pub const ENGINE_PHYSICS_CONTACTS_GATEWAY_ID: &str = "engine.physics.contacts";
pub const ENGINE_PHYSICS_CONSTRAINTS_GATEWAY_ID: &str = "engine.physics.constraints";
pub const ENGINE_UI_GATEWAY_ID: &str = "engine.ui";
pub const ENGINE_UI_TEXT_GATEWAY_ID: &str = "engine.ui.text";
pub const ENGINE_UI_DEBUG_GATEWAY_ID: &str = "engine.ui.debug";
pub const ENGINE_SCRIPTING_GATEWAY_ID: &str = "engine.scripting";
pub const ENGINE_SCENE_GATEWAY_ID: &str = "engine.scene";

#[inline]
pub fn is_engine_service_gateway_id(value: &str) -> bool {
    normalize_engine_gateway_id(value).is_some()
}

#[inline]
pub fn normalize_engine_gateway_id(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || !trimmed.starts_with(ENGINE_SERVICE_GATEWAY_PREFIX) {
        return None;
    }
    let mut out = Vec::new();
    for segment in trimmed.split('.') {
        let segment = segment.trim();
        if segment.is_empty() {
            return None;
        }
        out.push(segment.to_ascii_lowercase());
    }
    Some(out.join("."))
}

/// Declarative classification tags attached to services, backend routes and
/// gateway policy facts.
///
/// Tags intentionally stay string-based and data-driven: the host can classify
/// a route without adding a new enum variant or editing a central gateway list.
pub mod system_tag {
    /// Route belongs to the engine-facing gateway namespace.
    pub const ENGINE_DOMAIN: &str = "engine.domain";
    /// Route points at a backend provider implementation.
    pub const PROVIDER_BACKEND: &str = "provider.backend";
    /// Route belongs to a concrete provider implementation.
    ///
    /// Important: this tag does not make `engine.<domain>.<name>` a backend API.
    /// Provider names such as `engine.ui.<provider>` and `engine.render.<provider>` are
    /// implementation identities published as descriptor metadata while the
    /// actual routed API remains the root gateway (`engine.ui`, `engine.render`, ...).
    pub const PROVIDER_IMPLEMENTATION_ROUTE: &str = "provider.implementation_route";
    /// Gateway can be overridden by compatible providers.
    pub const OVERRIDE_OPEN: &str = "override.open";
    /// Gateway participates in profile-controlled provider selection.
    pub const OVERRIDE_PROFILE_CONTROLLED: &str = "override.profile_controlled";
    /// Gateway is a host trust-root and must reject plugin-owned providers.
    pub const OVERRIDE_LOCKED: &str = "override.locked";
    /// Gateway/service is part of the host trust root.
    pub const TRUST_ROOT: &str = "trust.root";
    /// Runtime-facing service, not an authoring-only/tooling surface.
    pub const RUNTIME: &str = "runtime";
    /// Provider requires an interactive/headful presentation environment.
    pub const HEADFUL: &str = "headful";
    /// Provider is suitable for a headless/server composition.
    pub const HEADLESS: &str = "headless";
    /// Provider participates in presentation/output.
    pub const PRESENTATION: &str = "presentation";
    /// Provider owns or requires GPU execution.
    pub const GPU: &str = "gpu";
    /// Provider owns or requires native windowing.
    pub const WINDOWING: &str = "windowing";
    /// Provider behavior is deterministic for equivalent inputs.
    pub const DETERMINISTIC: &str = "deterministic";
}

#[inline]
pub fn normalize_system_tag(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut out = Vec::new();
    for segment in trimmed.split('.') {
        let segment = segment.trim();
        if segment.is_empty() {
            return None;
        }
        out.push(segment.to_ascii_lowercase());
    }
    Some(out.join("."))
}

#[inline]
pub fn normalize_service_kind(value: &str) -> Option<String> {
    normalize_system_tag(value)
}

#[inline]
pub fn engine_gateway_domain(gateway_id: &str) -> Option<String> {
    normalize_engine_gateway_id(gateway_id)?
        .strip_prefix(ENGINE_SERVICE_GATEWAY_PREFIX)
        .map(str::to_owned)
}

#[inline]
pub fn engine_gateway_parent_id(gateway_id: &str) -> Option<String> {
    let normalized = normalize_engine_gateway_id(gateway_id)?;
    let mut parts: Vec<&str> = normalized.split('.').collect();
    if parts.len() <= 2 {
        return None;
    }
    parts.pop();
    Some(parts.join("."))
}

#[inline]
pub fn engine_gateway_root_id(gateway_id: &str) -> Option<String> {
    let normalized = normalize_engine_gateway_id(gateway_id)?;
    let mut parts = normalized.split('.');
    let prefix = parts.next()?;
    let domain = parts.next()?;
    Some(format!("{prefix}.{domain}"))
}

#[inline]
pub fn engine_gateway_depth(gateway_id: &str) -> Option<usize> {
    Some(normalize_engine_gateway_id(gateway_id)?.split('.').count())
}

/// Returns the normalized service-kind text implied by an `engine.*` gateway.
///
/// This is the data-driven path used by the gateway registry. It deliberately
/// does not consult `EngineServiceKind`, so new plugin/provider domains can be
/// introduced by descriptor metadata without editing a central enum.
#[inline]
pub fn service_kind_from_engine_gateway_id(gateway_id: &str) -> Option<String> {
    engine_gateway_domain(gateway_id).and_then(|domain| normalize_service_kind(&domain))
}

#[inline]
pub fn engine_gateway_matches_service_kind(gateway_id: &str, service_kind: &str) -> bool {
    service_kind_from_engine_gateway_id(gateway_id).as_deref()
        == normalize_service_kind(service_kind).as_deref()
}

#[inline]
pub fn engine_gateway_is_direct_child_of_service_kind(
    gateway_id: &str,
    service_kind: &str,
) -> bool {
    let Some(domain) = engine_gateway_domain(gateway_id) else {
        return false;
    };
    let Some(kind) = normalize_service_kind(service_kind) else {
        return false;
    };

    let domain_parts = domain.split('.').collect::<Vec<_>>();
    let kind_parts = kind.split('.').collect::<Vec<_>>();

    domain_parts.len() == kind_parts.len() + 1
        && domain_parts
            .iter()
            .zip(kind_parts.iter())
            .all(|(domain, kind)| domain == kind)
}
