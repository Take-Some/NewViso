#![forbid(unsafe_op_in_unsafe_fn)]

use abi_stable::sabi_trait;
use abi_stable::std_types::{RBox, ROption, RString, RVec};
use abi_stable::StableAbi;

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, StableAbi)]
pub enum EditorDisplayModeV1 {
    Both,
    EditorOnly,
    GameOnly,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, StableAbi)]
pub enum EditorToolIdV1 {
    Select,
    Translate,
    Rotate,
    Scale,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, StableAbi)]
pub enum EditorPlayModeV1 {
    Edit,
    Simulate,
    Play,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, StableAbi)]
pub enum EditorViewportModeV1 {
    Lit,
    Unlit,
    Wireframe,
    Collision,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, StableAbi)]
pub enum EditorWorkspacePresetV1 {
    Minimal,
    Editing,
    Debug,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, StableAbi)]
pub enum EditorPropertySectionIdV1 {
    Summary,
    Transform,
    Display,
    Gameplay,
    Collision,
    Primitive,
    Lighting,
    Material,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, StableAbi)]
pub enum EditorComponentSchemaIdV1 {
    Identity,
    Transform,
    Display,
    Gameplay,
    Collision,
    Primitive,
    DirectionalLight,
    PointLight,
    Material,
    ImportedAsset,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, StableAbi)]
pub enum EditorSelectionKindV1 {
    Actor,
    Primitive,
    DirectionalLight,
    PointLight,
    Player,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, StableAbi)]
pub enum EditorContextActionIdV1 {
    FrameSelection,
    Deselect,
    ToggleCollisionOverlay,
    EnterPlay,
    EnterSimulate,
    StopRuntime,
    SelectTool,
    MoveTool,
    RotateTool,
    ScaleTool,
    AddCollision,
    RemoveCollision,
    SpawnAssetHere,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, StableAbi)]
pub enum EditorPropertyFieldIdV1 {
    Name,
    Kind,
    Entity,
    DisplayMode,
    Position,
    RotationDeg,
    Scale,
    SnapTranslate,
    SnapRotate,
    SnapScale,
    GameplayRole,
    CollisionEnabled,
    CollisionDynamic,
    CollisionTrigger,
    CollisionShape,
    CollisionBoxExtents,
    CollisionSphereRadius,
    CollisionCapsuleRadius,
    CollisionCapsuleHalfHeight,
    PrimitiveKind,
    PrimitiveColor,
    LightAmbientColor,
    LightAmbientIntensity,
    LightColor,
    LightIntensity,
    LightRange,
    LightYawDeg,
    LightPitchDeg,
    MaterialAsset,
    MaterialBaseColor,
    MaterialMetallic,
    MaterialRoughness,
    MaterialEmissiveColor,
    MaterialEmissiveStrength,
    MaterialNormalScale,
    MaterialAoStrength,
    MaterialAlphaCutoff,
    ImportedAssetPath,
    ImportedAssetKind,
    ImportedAssetRepresentation,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, StableAbi)]
pub enum EditorFieldEditorKindV1 {
    ReadOnlyText,
    Vec3,
    Toggle,
    EnumChoice,
    Color3,
    Color4,
    Scalar,
    MaterialChoice,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, StableAbi)]
pub enum EditorAssetImportClassV1 {
    Model,
    Scene,
    Texture,
    Material,
    Unknown,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, StableAbi)]
pub enum EditorImportedAssetKindV1 {
    StaticMesh,
    SceneReference,
    TextureReference,
    MaterialReference,
    OpaqueReference,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, StableAbi)]
pub enum EditorImportedAssetRepresentationV1 {
    PrimitiveCube,
    PrimitivePlane,
    PrimitiveSphere,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, StableAbi)]
pub enum EditorImportedAssetAssemblyKindV1 {
    StaticMeshActor,
    SceneAnchor,
    TextureCard,
    MaterialPreviewSphere,
    OpaqueProxy,
}

#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq, StableAbi)]
pub struct EditorSelectionContextV1 {
    pub entity_stable_id: u64,
    pub name: RString,
    pub kind: EditorSelectionKindV1,
    pub has_collision: bool,
    pub display_mode: EditorDisplayModeV1,
    pub components: RVec<EditorComponentSchemaIdV1>,
}

#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq, StableAbi)]
pub struct EditorSurfaceContextV1 {
    pub play_mode: EditorPlayModeV1,
    pub runtime_active: bool,
    pub viewport_mode: EditorViewportModeV1,
    pub active_tool: EditorToolIdV1,
    pub camera_speed_label: RString,
    pub collision_overlay: bool,
    pub selection_count: u32,
    pub entity_count: u32,
    pub primary_selection: ROption<EditorSelectionContextV1>,
}

#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq, StableAbi)]
pub struct EditorPropertyFieldSchemaV1 {
    pub id: EditorPropertyFieldIdV1,
    pub label: RString,
    pub keywords: RString,
    pub editor: EditorFieldEditorKindV1,
    pub visible: bool,
    pub enabled: bool,
}

#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq, StableAbi)]
pub struct EditorContextActionSchemaV1 {
    pub id: EditorContextActionIdV1,
    pub label: RString,
    pub keywords: RString,
    pub enabled: bool,
    pub selected: bool,
}

#[repr(C)]
#[derive(Debug, Clone, PartialEq, StableAbi)]
pub struct EditorImportedAssetAssemblyDescriptorV1 {
    pub assembly: EditorImportedAssetAssemblyKindV1,
    pub primitive_key: RString,
    pub display_mode: EditorDisplayModeV1,
    pub with_collision: bool,
    pub dynamic_collision: bool,
}

#[repr(C)]
#[derive(Debug, Clone, PartialEq, StableAbi)]
pub struct EditorAssetImportDescriptorV1 {
    pub class: EditorAssetImportClassV1,
    pub representation: EditorImportedAssetRepresentationV1,
    pub import_kind: EditorImportedAssetKindV1,
    pub assembler_key: RString,
    pub assembly: EditorImportedAssetAssemblyDescriptorV1,
    pub default_scale: [f32; 3],
    pub tint: [f32; 4],
}

#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq, StableAbi)]
pub struct EditorAssetAssemblerDescriptorV1 {
    pub key: RString,
    pub label: RString,
    pub import_kind: EditorImportedAssetKindV1,
    pub assembly: EditorImportedAssetAssemblyKindV1,
}

#[repr(C)]
#[derive(Debug, Clone, PartialEq, StableAbi)]
pub struct EditorAssetSpawnContractV1 {
    pub logical_path: RString,
    pub actor_name: RString,
    pub import: EditorAssetImportDescriptorV1,
}

#[repr(C)]
#[derive(Debug, Clone, PartialEq, StableAbi)]
pub enum EditorCommandDescriptorV1 {
    NamedAction(RString),
    ContextAction(EditorContextActionIdV1),
    SpawnAsset(EditorAssetSpawnContractV1),
    SetTool(EditorToolIdV1),
    SetPlayMode(EditorPlayModeV1),
    SetWorkspacePreset(EditorWorkspacePresetV1),
    SetViewportMode(EditorViewportModeV1),
    PublishFrameSelection,
    PublishFrameAll,
    ToggleCollisionOverlay,
}

#[repr(C)]
#[derive(Debug, Clone, PartialEq, StableAbi)]
pub struct EditorCommandInvocationV1 {
    pub source: RString,
    pub command: EditorCommandDescriptorV1,
    pub surface: EditorSurfaceContextV1,
}

#[repr(C)]
#[derive(Debug, Clone, PartialEq, StableAbi)]
pub struct EditorCommandHandlerResultV1 {
    pub handled: bool,
    pub emitted: RVec<EditorCommandDescriptorV1>,
}

impl EditorCommandHandlerResultV1 {
    #[inline]
    pub fn pass() -> Self {
        Self {
            handled: false,
            emitted: RVec::new(),
        }
    }

    #[inline]
    pub fn handled_only() -> Self {
        Self {
            handled: true,
            emitted: RVec::new(),
        }
    }
}

#[sabi_trait]
pub trait EditorFieldFactoryProviderV1: Send + Sync {
    fn id(&self) -> RString;
    fn component(&self) -> ROption<EditorComponentSchemaIdV1>;
    fn section(&self) -> ROption<EditorPropertySectionIdV1>;
    fn build_fields(
        &self,
        surface: EditorSurfaceContextV1,
        selection: EditorSelectionContextV1,
        component: EditorComponentSchemaIdV1,
        section: EditorPropertySectionIdV1,
    ) -> RVec<EditorPropertyFieldSchemaV1>;
}

pub type EditorFieldFactoryProviderV1Dyn<'a> = EditorFieldFactoryProviderV1_TO<'a, RBox<()>>;

#[sabi_trait]
pub trait EditorContextActionProviderV1: Send + Sync {
    fn id(&self) -> RString;
    fn build_actions(
        &self,
        surface: EditorSurfaceContextV1,
        selection: ROption<EditorSelectionContextV1>,
    ) -> RVec<EditorContextActionSchemaV1>;
}

pub type EditorContextActionProviderV1Dyn<'a> = EditorContextActionProviderV1_TO<'a, RBox<()>>;

#[sabi_trait]
pub trait EditorAssetImportProviderV1: Send + Sync {
    fn id(&self) -> RString;
    fn infer_import(&self, logical_path: RString) -> ROption<EditorAssetImportDescriptorV1>;
}

pub type EditorAssetImportProviderV1Dyn<'a> = EditorAssetImportProviderV1_TO<'a, RBox<()>>;

#[sabi_trait]
pub trait EditorCommandHandlerV1: Send + Sync {
    fn id(&self) -> RString;
    fn handle_command(&self, invocation: EditorCommandInvocationV1)
        -> EditorCommandHandlerResultV1;
}

pub type EditorCommandHandlerV1Dyn<'a> = EditorCommandHandlerV1_TO<'a, RBox<()>>;

#[repr(C)]
#[derive(StableAbi)]
pub struct EditorExtensionsV1 {
    pub field_factories: RVec<EditorFieldFactoryProviderV1Dyn<'static>>,
    pub context_action_providers: RVec<EditorContextActionProviderV1Dyn<'static>>,
    pub asset_import_providers: RVec<EditorAssetImportProviderV1Dyn<'static>>,
    pub asset_assemblers: RVec<EditorAssetAssemblerDescriptorV1>,
    pub command_handlers: RVec<EditorCommandHandlerV1Dyn<'static>>,
}

impl EditorExtensionsV1 {
    #[inline]
    pub fn empty() -> Self {
        Self {
            field_factories: RVec::new(),
            context_action_providers: RVec::new(),
            asset_import_providers: RVec::new(),
            asset_assemblers: RVec::new(),
            command_handlers: RVec::new(),
        }
    }
}
