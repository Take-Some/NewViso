#![forbid(unsafe_op_in_unsafe_fn)]

//! Shared authoring kit for NewEngine plugins.
//!
//! This crate is intentionally tiny: it does not define a second ABI.
//! It reexports the central engine API and provides macros so runtime plugins
//! do not manually construct `PluginRootV1` or copy ABI boilerplate.

pub use abi_stable;
pub use newengine_plugin_api as plugin_api;
pub use newengine_plugin_api::*;

pub mod definition;

pub mod prelude {
    pub use crate::definition::*;
    pub use crate::plugin_api::*;
    pub use crate::{
        export_newengine_plugin, export_newengine_plugin_descriptor_v2,
        export_newengine_plugin_root, export_newengine_plugin_signature,
    };
}

/// Export a plugin root from explicitly declared create callbacks.
#[macro_export]
macro_rules! export_newengine_plugin_root {
    ($create:path) => {
        $crate::plugin_api::export_plugin_root!($create);
    };
    ($create:path, $ui_assets_v1:path) => {
        $crate::plugin_api::export_plugin_root!($create, $ui_assets_v1);
    };
    ($create:path, $ui_assets_v1:path, $editor_extensions_v1:path) => {
        $crate::plugin_api::export_plugin_root!($create, $ui_assets_v1, $editor_extensions_v1);
    };
}

/// Exports a first-party native V2 discovery descriptor. The descriptor function
/// must construct `PluginDescriptorV2` directly; V1 compatibility normalization
/// belongs to the host's legacy-plugin path, not production provider authoring.
#[macro_export]
macro_rules! export_newengine_plugin_descriptor_v2 {
    ($descriptor:path) => {
        $crate::plugin_api::export_plugin_descriptor_v2!($descriptor);
    };
}

/// Exports the lightweight signature consumed by descriptor-first plugin discovery.
///
/// This macro is separate from `export_newengine_plugin!` because several legacy
/// providers already define the symbol manually. New providers should use both macros.
#[macro_export]
macro_rules! export_newengine_plugin_signature {
    (
        id = $id:expr,
        name = $name:expr,
        kind = $kind:expr,
        phase = $phase:expr $(,)?
    ) => {
        $crate::export_newengine_plugin_signature!(
            id = $id,
            name = $name,
            version = env!("CARGO_PKG_VERSION"),
            kind = $kind,
            phase = $phase,
        );
    };
    (
        id = $id:expr,
        name = $name:expr,
        version = $version:expr,
        kind = $kind:expr,
        phase = $phase:expr $(,)?
    ) => {
        #[no_mangle]
        pub extern "C" fn newengine_plugin_signature_v1() -> $crate::plugin_api::PluginSignatureV1 {
            $crate::plugin_api::PluginSignatureV1 {
                id: $crate::abi_stable::std_types::RString::from($id),
                name: $crate::abi_stable::std_types::RString::from($name),
                version: $crate::abi_stable::std_types::RString::from($version),
                kind: $kind,
                bootstrap_phase: $phase,
            }
        }
    };
}

/// Export the common case where one module type implements `PluginModule`.
#[macro_export]
macro_rules! export_newengine_plugin {
    (module = $module:expr, editor_extensions_v1 = $editor_extensions:path $(, icon_png = $icon:path)? $(,)?) => {
        extern "C" fn create_module() -> $crate::plugin_api::PluginModuleDyn<'static> {
            $crate::plugin_api::PluginModule_TO::from_value(
                $module,
                $crate::abi_stable::sabi_trait::TD_Opaque,
            )
        }

        extern "C" fn ui_assets_v1() -> $crate::plugin_api::PluginUiAssetsV1 {
            $crate::export_newengine_plugin!(@ui_assets $($icon)?)
        }

        $crate::plugin_api::export_plugin_root!(
            create_module,
            ui_assets_v1,
            $editor_extensions
        );
    };

    (module = $module:expr $(, icon_png = $icon:path)? $(,)?) => {
        extern "C" fn create_module() -> $crate::plugin_api::PluginModuleDyn<'static> {
            $crate::plugin_api::PluginModule_TO::from_value(
                $module,
                $crate::abi_stable::sabi_trait::TD_Opaque,
            )
        }

        extern "C" fn ui_assets_v1() -> $crate::plugin_api::PluginUiAssetsV1 {
            $crate::export_newengine_plugin!(@ui_assets $($icon)?)
        }

        $crate::plugin_api::export_plugin_root!(create_module, ui_assets_v1);
    };

    (@ui_assets $icon:path) => {
        $crate::plugin_api::PluginUiAssetsV1::empty()
    };

    (@ui_assets) => {
        $crate::plugin_api::PluginUiAssetsV1::empty()
    };
}
