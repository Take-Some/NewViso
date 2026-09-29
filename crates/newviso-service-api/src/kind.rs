use serde::{Serialize, Serializer};

use crate::{normalize_engine_gateway_id, ENGINE_SERVICE_GATEWAY_PREFIX};

/// Engine-side vocabulary for service provider kinds accepted by the host.
///
/// Plugins do not need to import this enum or know the full set. They describe
/// themselves with string metadata such as `service_kind = "render"`; the
/// host validates that string against this vocabulary and ignores unknown kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EngineServiceKind {
    Assets,
    AssetVfs,
    AssetTypes,
    AssetInspect,
    AssetEdit,
    AssetPackages,
    AssetListFiles,
    AssetUid,
    AssetDependencies,
    AssetImportQueue,
    AssetPackageWriter,
    AssetMaps,
    AssetValidation,
    AssetUi,
    Materials,
    Definitions,
    AssetGraph,
    Time,
    Schema,
    Animation,
    Navigation,
    Ai,
    Tags,
    Tasks,
    Scripting,
    Audio,
    Render,
    RenderEffects,
    RenderMaterials,
    Model,
    ModelSkeletons,
    ModelMaterials,
    ModelCollisions,
    Camera,
    CameraModes,
    CameraAnimations,
    Scene,
    Physics,
    PhysicsContacts,
    PhysicsConstraints,
    Input,
    InputBindings,
    InputActions,
    InputContexts,
    Ui,
    UiText,
    UiDebug,
    Logging,
    Loading,
    Threading,
    Platform,
    Ecs,
    Entity,
    PluginHost,
    Abi,
    GatewayRegistry,
    Security,
    SchedulerCore,
    CapabilityValidator,
}

impl Serialize for EngineServiceKind {
    #[inline]
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl AsRef<str> for EngineServiceKind {
    #[inline]
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl EngineServiceKind {
    #[inline]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Assets => "assets",
            Self::AssetVfs => "assets.vfs",
            Self::AssetTypes => "assets.types",
            Self::AssetInspect => "assets.inspect",
            Self::AssetEdit => "assets.edit",
            Self::AssetPackages => "assets.packages",
            Self::AssetListFiles => "assets.listfiles",
            Self::AssetUid => "assets.uid",
            Self::AssetDependencies => "assets.dependencies",
            Self::AssetImportQueue => "assets.import_queue",
            Self::AssetPackageWriter => "assets.package_writer",
            Self::AssetMaps => "assets.maps",
            Self::AssetValidation => "assets.validation",
            Self::AssetUi => "assets.ui",
            Self::Materials => "assets.materials",
            Self::Definitions => "assets.definitions",
            Self::AssetGraph => "assets.graph",
            Self::Time => "time",
            Self::Schema => "schema",
            Self::Animation => "animation",
            Self::Navigation => "navigation",
            Self::Ai => "ai",
            Self::Tags => "tags",
            Self::Tasks => "tasks",
            Self::Scripting => "scripting",
            Self::Audio => "audio",
            Self::Render => "render",
            Self::RenderEffects => "render.effects",
            Self::RenderMaterials => "render.materials",
            Self::Model => "assets.models",
            Self::ModelSkeletons => "assets.models.skeletons",
            Self::ModelMaterials => "assets.models.materials",
            Self::ModelCollisions => "assets.models.collisions",
            Self::Camera => "camera",
            Self::CameraModes => "camera.modes",
            Self::CameraAnimations => "camera.animations",
            Self::Scene => "scene",
            Self::Physics => "physics",
            Self::PhysicsContacts => "physics.contacts",
            Self::PhysicsConstraints => "physics.constraints",
            Self::Input => "input",
            Self::InputBindings => "input.bindings",
            Self::InputActions => "input.actions",
            Self::InputContexts => "input.contexts",
            Self::Ui => "ui",
            Self::UiText => "ui.text",
            Self::UiDebug => "ui.debug",
            Self::Logging => "logging",
            Self::Loading => "loading",
            Self::Threading => "threading",
            Self::Platform => "platform",
            Self::Ecs => "ecs",
            Self::Entity => "entity",
            Self::PluginHost => "plugin_host",
            Self::Abi => "abi",
            Self::GatewayRegistry => "gateway_registry",
            Self::Security => "security",
            Self::SchedulerCore => "scheduler.core",
            Self::CapabilityValidator => "capability_validator",
        }
    }

    #[inline]
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "assets" => Some(Self::Assets),
            "assets.vfs" | "assets_vfs" => Some(Self::AssetVfs),
            "assets.types" | "assets_types" => Some(Self::AssetTypes),
            "assets.inspect" | "assets_inspect" => Some(Self::AssetInspect),
            "assets.edit" | "assets_edit" => Some(Self::AssetEdit),
            "assets.packages" | "assets_packages" => Some(Self::AssetPackages),
            "assets.listfiles" | "assets_listfiles" | "assets.list_files" | "assets_list_files" => {
                Some(Self::AssetListFiles)
            }
            "assets.uid" | "assets_uid" => Some(Self::AssetUid),
            "assets.dependencies" | "assets_dependencies" => Some(Self::AssetDependencies),
            "assets.import_queue" | "assets_import_queue" | "assets.import-queue" => {
                Some(Self::AssetImportQueue)
            }
            "assets.package_writer" | "assets_package_writer" | "assets.package-writer" => {
                Some(Self::AssetPackageWriter)
            }
            "assets.maps" | "assets_maps" => Some(Self::AssetMaps),
            "assets.validation" | "assets_validation" => Some(Self::AssetValidation),
            "assets.ui" | "assets_ui" => Some(Self::AssetUi),
            "assets.materials" | "assets_materials" => Some(Self::Materials),
            "assets.definitions" | "assets_definitions" => Some(Self::Definitions),
            "assets.graph" | "assets_graph" | "assets-graph" => Some(Self::AssetGraph),
            "time" => Some(Self::Time),
            "schema" => Some(Self::Schema),
            "animation" => Some(Self::Animation),
            "navigation" => Some(Self::Navigation),
            "ai" => Some(Self::Ai),
            "tags" => Some(Self::Tags),
            "tasks" => Some(Self::Tasks),
            "scripting" => Some(Self::Scripting),
            "audio" => Some(Self::Audio),
            "render" => Some(Self::Render),
            "render.effects" | "render_effects" => Some(Self::RenderEffects),
            "render.materials" | "render_materials" => Some(Self::RenderMaterials),
            "assets.models" | "assets_models" => Some(Self::Model),
            "assets.models.skeletons" | "assets_models_skeletons" => Some(Self::ModelSkeletons),
            "assets.models.materials" | "assets_models_materials" => Some(Self::ModelMaterials),
            "assets.models.collisions" | "assets_models_collisions" => Some(Self::ModelCollisions),
            "camera" => Some(Self::Camera),
            "camera.modes" | "camera_modes" => Some(Self::CameraModes),
            "camera.animations" | "camera_animations" => Some(Self::CameraAnimations),
            "scene" => Some(Self::Scene),
            "physics" => Some(Self::Physics),
            "physics.contacts" | "physics_contacts" => Some(Self::PhysicsContacts),
            "physics.constraints" | "physics_constraints" => Some(Self::PhysicsConstraints),
            "input" => Some(Self::Input),
            "input.bindings" | "input_bindings" => Some(Self::InputBindings),
            "input.actions" | "input_actions" => Some(Self::InputActions),
            "input.contexts" | "input_contexts" => Some(Self::InputContexts),
            "ui" => Some(Self::Ui),
            "ui.text" | "ui_text" => Some(Self::UiText),
            "ui.debug" | "ui_debug" => Some(Self::UiDebug),
            "logging" => Some(Self::Logging),
            "loading" => Some(Self::Loading),
            "threading" => Some(Self::Threading),
            "platform" => Some(Self::Platform),
            "ecs" => Some(Self::Ecs),
            "entity" => Some(Self::Entity),
            "plugin_host" | "plugin-host" | "plugin.host" => Some(Self::PluginHost),
            "abi" => Some(Self::Abi),
            "gateway_registry" | "gateway-registry" | "gateway.registry" => {
                Some(Self::GatewayRegistry)
            }
            "security" => Some(Self::Security),
            "scheduler.core" | "scheduler_core" | "scheduler-core" => Some(Self::SchedulerCore),
            "capability_validator" | "capability-validator" | "capability.validator" => {
                Some(Self::CapabilityValidator)
            }
            _ => None,
        }
    }

    /// Returns the direct parent domain for third-level extension domains.
    ///
    /// Example: `input.bindings -> input`, `render.effects -> render`.
    #[inline]
    pub const fn parent(self) -> Option<Self> {
        match self {
            Self::AssetVfs
            | Self::AssetTypes
            | Self::AssetInspect
            | Self::AssetEdit
            | Self::AssetPackages
            | Self::AssetListFiles
            | Self::AssetUid
            | Self::AssetDependencies
            | Self::AssetImportQueue
            | Self::AssetPackageWriter
            | Self::AssetMaps
            | Self::AssetValidation
            | Self::AssetUi
            | Self::Materials
            | Self::Definitions
            | Self::AssetGraph
            | Self::Model => Some(Self::Assets),
            Self::RenderEffects | Self::RenderMaterials => Some(Self::Render),
            Self::ModelSkeletons | Self::ModelMaterials | Self::ModelCollisions => {
                Some(Self::Model)
            }
            Self::CameraModes | Self::CameraAnimations => Some(Self::Camera),
            Self::PhysicsContacts | Self::PhysicsConstraints => Some(Self::Physics),
            Self::InputBindings | Self::InputActions | Self::InputContexts => Some(Self::Input),
            Self::UiText | Self::UiDebug => Some(Self::Ui),
            _ => None,
        }
    }

    #[inline]
    pub const fn root(self) -> Self {
        match self.parent() {
            Some(parent) => parent,
            None => self,
        }
    }

    #[inline]
    pub const fn domain_depth(self) -> u8 {
        match self.parent() {
            Some(_) => 3,
            None => 2,
        }
    }

    #[inline]
    pub const fn engine_gateway_id(self) -> &'static str {
        match self {
            Self::Assets => crate::ENGINE_ASSETS_GATEWAY_ID,
            Self::AssetVfs => crate::ENGINE_ASSETS_VFS_GATEWAY_ID,
            Self::AssetTypes => crate::ENGINE_ASSETS_TYPES_GATEWAY_ID,
            Self::AssetInspect => crate::ENGINE_ASSETS_INSPECT_GATEWAY_ID,
            Self::AssetEdit => crate::ENGINE_ASSETS_EDIT_GATEWAY_ID,
            Self::AssetPackages => crate::ENGINE_ASSETS_PACKAGES_GATEWAY_ID,
            Self::AssetListFiles => crate::ENGINE_ASSETS_LISTFILES_GATEWAY_ID,
            Self::AssetUid => crate::ENGINE_ASSETS_UID_GATEWAY_ID,
            Self::AssetDependencies => crate::ENGINE_ASSETS_DEPENDENCIES_GATEWAY_ID,
            Self::AssetImportQueue => crate::ENGINE_ASSETS_IMPORT_QUEUE_GATEWAY_ID,
            Self::AssetPackageWriter => crate::ENGINE_ASSETS_PACKAGE_WRITER_GATEWAY_ID,
            Self::AssetMaps => crate::ENGINE_ASSETS_MAPS_GATEWAY_ID,
            Self::AssetValidation => crate::ENGINE_ASSETS_VALIDATION_GATEWAY_ID,
            Self::AssetUi => crate::ENGINE_ASSETS_UI_GATEWAY_ID,
            Self::Materials => crate::ENGINE_ASSETS_MATERIALS_GATEWAY_ID,
            Self::Definitions => crate::ENGINE_ASSETS_DEFINITIONS_GATEWAY_ID,
            Self::AssetGraph => crate::ENGINE_ASSETS_GRAPH_GATEWAY_ID,
            Self::Time => "engine.time",
            Self::Schema => "engine.schema",
            Self::Animation => "engine.animation",
            Self::Navigation => "engine.navigation",
            Self::Ai => "engine.ai",
            Self::Tags => "engine.tags",
            Self::Tasks => "engine.tasks",
            Self::Scripting => crate::ENGINE_SCRIPTING_GATEWAY_ID,
            Self::Audio => "engine.audio",
            Self::Render => crate::ENGINE_RENDER_GATEWAY_ID,
            Self::RenderEffects => crate::ENGINE_RENDER_EFFECTS_GATEWAY_ID,
            Self::RenderMaterials => crate::ENGINE_RENDER_MATERIALS_GATEWAY_ID,
            Self::Model => crate::ENGINE_ASSETS_MODELS_GATEWAY_ID,
            Self::ModelSkeletons => crate::ENGINE_ASSETS_MODELS_SKELETONS_GATEWAY_ID,
            Self::ModelMaterials => crate::ENGINE_ASSETS_MODELS_MATERIALS_GATEWAY_ID,
            Self::ModelCollisions => crate::ENGINE_ASSETS_MODELS_COLLISIONS_GATEWAY_ID,
            Self::Camera => "engine.camera",
            Self::CameraModes => "engine.camera.modes",
            Self::CameraAnimations => "engine.camera.animations",
            Self::Scene => crate::ENGINE_SCENE_GATEWAY_ID,
            Self::Physics => crate::ENGINE_PHYSICS_GATEWAY_ID,
            Self::PhysicsContacts => crate::ENGINE_PHYSICS_CONTACTS_GATEWAY_ID,
            Self::PhysicsConstraints => crate::ENGINE_PHYSICS_CONSTRAINTS_GATEWAY_ID,
            Self::Input => "engine.input",
            Self::InputBindings => "engine.input.bindings",
            Self::InputActions => "engine.input.actions",
            Self::InputContexts => "engine.input.contexts",
            Self::Ui => crate::ENGINE_UI_GATEWAY_ID,
            Self::UiText => crate::ENGINE_UI_TEXT_GATEWAY_ID,
            Self::UiDebug => crate::ENGINE_UI_DEBUG_GATEWAY_ID,
            Self::Logging => "engine.logging",
            Self::Loading => "engine.ui.loading",
            Self::Threading => "engine.threading",
            Self::Platform => "engine.platform",
            Self::Ecs => "engine.ecs",
            Self::Entity => "engine.entity",
            Self::PluginHost => "engine.plugin_host",
            Self::Abi => "engine.abi",
            Self::GatewayRegistry => "engine.gateway_registry",
            Self::Security => "engine.security",
            Self::SchedulerCore => "engine.scheduler.core",
            Self::CapabilityValidator => "engine.capability_validator",
        }
    }

    #[inline]
    pub fn matches_engine_gateway_id(self, gateway_id: &str) -> bool {
        self.engine_gateway_id()
            == normalize_engine_gateway_id(gateway_id)
                .as_deref()
                .unwrap_or("")
    }

    #[inline]
    pub fn parse_engine_gateway_id(gateway_id: &str) -> Option<Self> {
        let normalized = normalize_engine_gateway_id(gateway_id)?;
        let domain = normalized.strip_prefix(ENGINE_SERVICE_GATEWAY_PREFIX)?;
        Self::parse(domain)
    }
}
