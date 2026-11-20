#![forbid(unsafe_op_in_unsafe_fn)]

use crate::AssetFileTypeDescriptor;
use newengine_schema_api::{
    SchemaPatchDtoV1, SchemaPatchOperationV1, SchemaPropertyDescriptorV1, SchemaTransactionDtoV1,
    SchemaTypeDescriptorV1,
};

/// Provider-routed asset document inspection gateway.
///
/// Asset Browser and editor UI surfaces call this gateway to obtain normalized,
/// schema-driven DTOs. They must not parse `.ytyp/.ydd/.ytd/.nemat/.nepak` locally.
pub const ENGINE_ASSETS_INSPECT_SERVICE_ID: &str = "engine.assets.inspect";
pub const ASSETS_INSPECT_SERVICE_ID: &str = "assets.inspect.api";
pub const ASSETS_INSPECT_BACKEND_CAPABILITY_ID: &str = "assets.inspect.backend";
pub const ASSETS_INSPECT_RUNTIME_CONTRACT: &str = "newengine.assets.inspect.v1";

/// Provider-routed asset document mutation gateway.
///
/// UI emits `AssetPatch` DTOs here. The selected provider validates, writes back,
/// repacks or rejects through explicit writer capabilities.
pub const ENGINE_ASSETS_EDIT_SERVICE_ID: &str = "engine.assets.edit";
pub const ASSETS_EDIT_SERVICE_ID: &str = "assets.edit.api";
pub const ASSETS_EDIT_BACKEND_CAPABILITY_ID: &str = "assets.edit.backend";
pub const ASSETS_EDIT_RUNTIME_CONTRACT: &str = "newengine.assets.edit.v1";

pub mod asset_document_action_id {
    pub const ADD_ENTRY: &str = "asset.document.add_entry";
    pub const DELETE: &str = "asset.document.delete";
    pub const RENAME: &str = "asset.document.rename";
    pub const SAVE: &str = "asset.document.save";
    pub const REBUILD: &str = "asset.document.rebuild";
    pub const DISCARD: &str = "asset.document.discard_staged";
}

pub mod asset_inspect_method {
    pub const INFO_JSON: &str = newviso_service_api::SERVICE_METHOD_INFO_JSON;
    pub const INVOKE_JSON: &str = newviso_service_api::SERVICE_METHOD_INVOKE_JSON;
    pub const SHUTDOWN_V1: &str = newviso_service_api::SERVICE_METHOD_SHUTDOWN_V1;
    pub const INSPECT_DOCUMENT_JSON_V1: &str = "assets.inspect.document_json_v1";
    pub const PREVIEW_JSON_V1: &str = "assets.inspect.preview_json_v1";
    pub const VALIDATE_REF_JSON_V1: &str = "assets.inspect.validate_ref_json_v1";
}

pub mod asset_edit_method {
    pub const INFO_JSON: &str = newviso_service_api::SERVICE_METHOD_INFO_JSON;
    pub const INVOKE_JSON: &str = newviso_service_api::SERVICE_METHOD_INVOKE_JSON;
    pub const SHUTDOWN_V1: &str = newviso_service_api::SERVICE_METHOD_SHUTDOWN_V1;
    pub const VALIDATE_PATCH_JSON_V1: &str = "assets.edit.validate_patch_json_v1";
    pub const APPLY_PATCH_JSON_V1: &str = "assets.edit.apply_patch_json_v1";
    pub const STAGE_PATCH_JSON_V1: &str = "assets.edit.stage_patch_json_v1";
    pub const REBUILD_JSON_V1: &str = "assets.edit.rebuild_json_v1";
    pub const DISCARD_STAGED_JSON_V1: &str = "assets.edit.discard_staged_json_v1";
    pub const DIRTY_STATE_JSON_V1: &str = "assets.edit.dirty_state_json_v1";
}

pub const ASSETS_INSPECT_SERVICE_METHODS: &[&str] = &[
    asset_inspect_method::INFO_JSON,
    asset_inspect_method::INVOKE_JSON,
    asset_inspect_method::SHUTDOWN_V1,
    asset_inspect_method::INSPECT_DOCUMENT_JSON_V1,
    asset_inspect_method::PREVIEW_JSON_V1,
    asset_inspect_method::VALIDATE_REF_JSON_V1,
];

pub const ASSETS_EDIT_SERVICE_METHODS: &[&str] = &[
    asset_edit_method::INFO_JSON,
    asset_edit_method::INVOKE_JSON,
    asset_edit_method::SHUTDOWN_V1,
    asset_edit_method::VALIDATE_PATCH_JSON_V1,
    asset_edit_method::APPLY_PATCH_JSON_V1,
    asset_edit_method::STAGE_PATCH_JSON_V1,
    asset_edit_method::REBUILD_JSON_V1,
    asset_edit_method::DISCARD_STAGED_JSON_V1,
    asset_edit_method::DIRTY_STATE_JSON_V1,
];

pub const ASSETS_INSPECT_BACKEND_SERVICE_SPEC: newviso_service_api::BackendServiceSpec =
    newviso_service_api::BackendServiceSpec::new(
        "assets.inspect",
        ENGINE_ASSETS_INSPECT_SERVICE_ID,
        ASSETS_INSPECT_SERVICE_ID,
        ASSETS_INSPECT_BACKEND_CAPABILITY_ID,
    );

pub const ASSETS_EDIT_BACKEND_SERVICE_SPEC: newviso_service_api::BackendServiceSpec =
    newviso_service_api::BackendServiceSpec::new(
        "assets.edit",
        ENGINE_ASSETS_EDIT_SERVICE_ID,
        ASSETS_EDIT_SERVICE_ID,
        ASSETS_EDIT_BACKEND_CAPABILITY_ID,
    );

pub const ASSETS_INSPECT_RUNTIME_CONTRACT_SPEC: newviso_service_api::RuntimeServiceContractSpec =
    newviso_service_api::RuntimeServiceContractSpec::new(
        ENGINE_ASSETS_INSPECT_SERVICE_ID,
        ASSETS_INSPECT_RUNTIME_CONTRACT,
        ASSETS_INSPECT_SERVICE_METHODS,
    );

pub const ASSETS_EDIT_RUNTIME_CONTRACT_SPEC: newviso_service_api::RuntimeServiceContractSpec =
    newviso_service_api::RuntimeServiceContractSpec::new(
        ENGINE_ASSETS_EDIT_SERVICE_ID,
        ASSETS_EDIT_RUNTIME_CONTRACT,
        ASSETS_EDIT_SERVICE_METHODS,
    );

pub const ASSETS_INSPECT_RUNTIME_REQUIREMENT_SPEC:
    newviso_service_api::RuntimeServiceRequirementSpec =
    newviso_service_api::RuntimeServiceRequirementSpec::new(
        ASSETS_INSPECT_RUNTIME_CONTRACT_SPEC,
        Some(ASSETS_INSPECT_BACKEND_CAPABILITY_ID),
        Some("NEWENGINE_REQUIRE_ASSETS_INSPECT"),
    );

pub const ASSETS_EDIT_RUNTIME_REQUIREMENT_SPEC:
    newviso_service_api::RuntimeServiceRequirementSpec =
    newviso_service_api::RuntimeServiceRequirementSpec::new(
        ASSETS_EDIT_RUNTIME_CONTRACT_SPEC,
        Some(ASSETS_EDIT_BACKEND_CAPABILITY_ID),
        Some("NEWENGINE_REQUIRE_ASSETS_EDIT"),
    );

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct AssetDocumentRequest {
    /// Logical VFS reference: `path/file.ytyp`, `path/file.ytyp@Entry`, `.ytd@texture`, etc.
    pub asset_ref: String,
    /// Optional provider-specific selection path inside the document.
    pub selection: Option<String>,
    /// Human/UI context that requested the document, e.g. `asset_browser.right_edit_window`.
    pub requester: String,
    /// Schema-level patch used by engine.schema for validation/default/undo planning.
    pub schema_patch: Option<SchemaPatchDtoV1>,
    /// Editor transaction envelope. UI/history systems use this for undo/redo without inventing domain operations.
    pub transaction: Option<SchemaTransactionDtoV1>,
}

impl Default for AssetDocumentRequest {
    fn default() -> Self {
        Self {
            asset_ref: String::new(),
            selection: None,
            requester: "engine.editor".to_owned(),
            schema_patch: None,
            transaction: None,
        }
    }
}

impl AssetDocumentRequest {
    #[inline]
    pub fn new(asset_ref: impl Into<String>) -> Self {
        Self {
            asset_ref: asset_ref.into(),
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct AssetDocumentPreview {
    pub kind: String,
    pub icon: String,
    pub thumbnail_ref: String,
    pub summary: String,
}

impl Default for AssetDocumentPreview {
    fn default() -> Self {
        Self {
            kind: "provider_declared".to_owned(),
            icon: String::new(),
            thumbnail_ref: String::new(),
            summary: String::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct AssetDocumentText {
    pub encoding: String,
    pub language: String,
    pub content: String,
    pub byte_len: u64,
    pub line_count: usize,
    pub truncated: bool,
    pub editable: bool,
}

impl Default for AssetDocumentText {
    fn default() -> Self {
        Self {
            encoding: "utf-8".to_owned(),
            language: "text".to_owned(),
            content: String::new(),
            byte_len: 0,
            line_count: 0,
            truncated: false,
            editable: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AssetDocumentDiagnosticSeverity {
    #[default]
    Info,
    Warning,
    Error,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct AssetDocumentDiagnostic {
    pub severity: AssetDocumentDiagnosticSeverity,
    pub code: String,
    pub message: String,
    pub path: Option<String>,
}

impl Default for AssetDocumentDiagnostic {
    fn default() -> Self {
        Self {
            severity: AssetDocumentDiagnosticSeverity::Info,
            code: String::new(),
            message: String::new(),
            path: None,
        }
    }
}

impl AssetDocumentDiagnostic {
    #[inline]
    pub fn info(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: AssetDocumentDiagnosticSeverity::Info,
            code: code.into(),
            message: message.into(),
            path: None,
        }
    }

    #[inline]
    pub fn warn(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: AssetDocumentDiagnosticSeverity::Warning,
            code: code.into(),
            message: message.into(),
            path: None,
        }
    }

    #[inline]
    pub fn error(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: AssetDocumentDiagnosticSeverity::Error,
            code: code.into(),
            message: message.into(),
            path: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct AssetDocumentField {
    pub id: String,
    pub label: String,
    pub value_kind: String,
    pub value: serde_json::Value,
    pub editable: bool,
    pub required: bool,
    pub help: String,
    pub source_pointer: String,
    pub enum_values: Vec<String>,
    /// Canonical schema property descriptor used by Inspector, Asset Edit Window and scripting bind generation.
    pub schema_property: Option<SchemaPropertyDescriptorV1>,
}

impl Default for AssetDocumentField {
    fn default() -> Self {
        Self {
            id: String::new(),
            label: String::new(),
            value_kind: "string".to_owned(),
            value: serde_json::Value::Null,
            editable: false,
            required: false,
            help: String::new(),
            source_pointer: String::new(),
            enum_values: Vec::new(),
            schema_property: None,
        }
    }
}

impl AssetDocumentField {
    #[inline]
    pub fn readonly(
        id: impl Into<String>,
        label: impl Into<String>,
        value: impl Into<serde_json::Value>,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            value: value.into(),
            ..Self::default()
        }
    }

    #[inline]
    pub fn editable(
        id: impl Into<String>,
        label: impl Into<String>,
        value: impl Into<serde_json::Value>,
        value_kind: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            value: value.into(),
            value_kind: value_kind.into(),
            editable: true,
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, Default)]
#[serde(default)]
pub struct AssetDocumentSection {
    pub id: String,
    pub title: String,
    pub fields: Vec<AssetDocumentField>,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct AssetDocument {
    pub schema: String,
    pub asset_ref: String,
    pub title: String,
    pub icon: String,
    pub document_kind: String,
    pub asset_kind: String,
    pub content_kind: Option<u32>,
    pub semantic_gateway: String,
    pub provider_service: String,
    pub inspect_contract: String,
    pub edit_contract: String,
    /// Canonical type descriptor for property-driven editor rendering.
    pub schema_type: Option<SchemaTypeDescriptorV1>,
    /// Registry contract that owns validation/default-value semantics for this document.
    pub schema_contract: String,
    /// Compatibility projection for old consumers; mirrors `can_apply_patch`.
    pub editable: bool,
    /// Provider can expose editable field schema in `sections`.
    pub editable_fields_available: bool,
    /// Concrete writer route/capability is available for Apply/Save.
    pub can_apply_patch: bool,
    pub write_owner: String,
    pub writer_capability: String,
    pub dirty: bool,
    pub preview: AssetDocumentPreview,
    /// Provider-decoded UTF-8 document payload for text-editor consumers.
    /// The inspector never reads source files directly; this projection is
    /// populated through engine.assets text/VFS routing.
    pub text: Option<AssetDocumentText>,
    pub descriptor: Option<AssetFileTypeDescriptor>,
    /// Provider-declared document actions such as Add/Delete/Rename/Save.
    ///
    /// UI renders these as toolbar/context-menu commands and dispatches the
    /// embedded `AssetPatch` DTO through `engine.assets.edit`. This keeps
    /// Content Browser free of format-specific mutation branches.
    pub actions: Vec<AssetDocumentAction>,
    pub sections: Vec<AssetDocumentSection>,
    pub diagnostics: Vec<AssetDocumentDiagnostic>,
}

impl Default for AssetDocument {
    fn default() -> Self {
        Self {
            schema: "newengine.assets.document.v1".to_owned(),
            asset_ref: String::new(),
            title: String::new(),
            icon: String::new(),
            document_kind: "asset_document".to_owned(),
            asset_kind: String::new(),
            content_kind: None,
            semantic_gateway: String::new(),
            provider_service: String::new(),
            inspect_contract: ASSETS_INSPECT_RUNTIME_CONTRACT.to_owned(),
            edit_contract: String::new(),
            schema_type: None,
            schema_contract: newengine_schema_api::SCHEMA_RUNTIME_CONTRACT.to_owned(),
            editable: false,
            editable_fields_available: false,
            can_apply_patch: false,
            write_owner: "missing format writer provider".to_owned(),
            writer_capability: String::new(),
            dirty: false,
            preview: AssetDocumentPreview::default(),
            text: None,
            descriptor: None,
            actions: Vec::new(),
            sections: Vec::new(),
            diagnostics: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct AssetDocumentAction {
    /// Stable provider-owned action id, for example `asset.document.delete_entry`.
    pub id: String,
    pub label: String,
    pub tooltip: String,
    pub enabled: bool,
    pub disabled_reason: String,
    /// True when the command needs a dialog/schema payload before a patch can be emitted.
    pub requires_input: bool,
    pub target_gateway: String,
    pub method: String,
    /// Provider-built patch template. UI may fill explicit user input later,
    /// but it must not invent format operations by extension.
    pub patch_template: Option<AssetPatch>,
    /// Optional schema for dialog/input payloads required before a patch can be emitted.
    pub input_schema: Option<SchemaTypeDescriptorV1>,
}

impl Default for AssetDocumentAction {
    fn default() -> Self {
        Self {
            id: String::new(),
            label: String::new(),
            tooltip: String::new(),
            enabled: false,
            disabled_reason: String::new(),
            requires_input: false,
            target_gateway: ENGINE_ASSETS_EDIT_SERVICE_ID.to_owned(),
            method: asset_edit_method::APPLY_PATCH_JSON_V1.to_owned(),
            patch_template: None,
            input_schema: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct AssetPatchOperation {
    pub op: String,
    pub path: String,
    pub value: serde_json::Value,
    pub old_value: Option<serde_json::Value>,
    /// Canonical operation mirrored into schema validation/undo-redo DTOs.
    pub schema_operation: Option<SchemaPatchOperationV1>,
}

impl Default for AssetPatchOperation {
    fn default() -> Self {
        Self {
            op: "replace".to_owned(),
            path: String::new(),
            value: serde_json::Value::Null,
            old_value: None,
            schema_operation: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct AssetPatch {
    pub schema: String,
    pub asset_ref: String,
    pub provider_service: String,
    pub edit_contract: String,
    pub operations: Vec<AssetPatchOperation>,
    pub requester: String,
    /// Schema-level patch used by engine.schema for validation/default/undo planning.
    pub schema_patch: Option<SchemaPatchDtoV1>,
    /// Editor transaction envelope. UI/history systems use this for undo/redo without inventing domain operations.
    pub transaction: Option<SchemaTransactionDtoV1>,
}

impl Default for AssetPatch {
    fn default() -> Self {
        Self {
            schema: "newengine.assets.patch.v1".to_owned(),
            asset_ref: String::new(),
            provider_service: String::new(),
            edit_contract: ASSETS_EDIT_RUNTIME_CONTRACT.to_owned(),
            operations: Vec::new(),
            requester: "engine.editor".to_owned(),
            schema_patch: None,
            transaction: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct AssetPatchResult {
    pub schema: String,
    pub asset_ref: String,
    pub accepted: bool,
    pub written: bool,
    pub dirty: bool,
    /// Number of provider-owned staged mutations waiting for rebuild/commit.
    pub staged_operations: usize,
    /// Provider-owned staged patches used by tooling to project a virtual container view.
    pub staged_patches: Vec<AssetPatch>,
    pub diagnostics: Vec<AssetDocumentDiagnostic>,
}

impl Default for AssetPatchResult {
    fn default() -> Self {
        Self {
            schema: "newengine.assets.patch_result.v1".to_owned(),
            asset_ref: String::new(),
            accepted: false,
            written: false,
            dirty: false,
            staged_operations: 0,
            staged_patches: Vec::new(),
            diagnostics: Vec::new(),
        }
    }
}
