#![forbid(unsafe_op_in_unsafe_fn)]

use abi_stable::library::RootModule;
use abi_stable::sabi_types::VersionStrings;
use abi_stable::StableAbi;

use crate::editor::EditorExtensionsV1;
use crate::module::PluginModuleDyn;
use crate::ui::PluginUiAssetsV1;

pub const PLUGIN_ROOT_SYMBOL_NAME: &str = "newengine_plugin_root_v1";
pub const LEGACY_PLUGIN_ROOT_SYMBOL_NAME: &str = "export_plugin_root";
pub const PLUGIN_ROOT_SYMBOL_BYTES: &[u8] = b"newengine_plugin_root_v1";
pub const PLUGIN_ROOT_SYMBOL_BYTES_NUL: &[u8] = b"newengine_plugin_root_v1\0";
pub const LEGACY_PLUGIN_ROOT_SYMBOL_BYTES: &[u8] = b"export_plugin_root";
pub const LEGACY_PLUGIN_ROOT_SYMBOL_BYTES_NUL: &[u8] = b"export_plugin_root\0";

pub const PLUGIN_DESCRIPTOR_V2_SYMBOL_NAME: &str = "newengine_plugin_descriptor_v2";
pub const PLUGIN_DESCRIPTOR_V2_SYMBOL_BYTES: &[u8] = b"newengine_plugin_descriptor_v2";
pub const PLUGIN_DESCRIPTOR_V2_SYMBOL_BYTES_NUL: &[u8] = b"newengine_plugin_descriptor_v2\0";
pub type PluginDescriptorV2Fn = extern "C" fn() -> crate::capability::PluginDescriptorV2;

#[repr(C)]
#[derive(StableAbi)]
#[sabi(kind(Prefix(prefix_ref = PluginRootV1Ref)))]
pub struct PluginRootV1 {
    #[sabi(last_prefix_field)]
    pub create: extern "C" fn() -> PluginModuleDyn<'static>,

    pub ui_assets_v1: extern "C" fn() -> PluginUiAssetsV1,

    pub editor_extensions_v1: extern "C" fn() -> EditorExtensionsV1,
}

impl RootModule for PluginRootV1Ref {
    abi_stable::declare_root_module_statics! { PluginRootV1Ref }

    const BASE_NAME: &'static str = PLUGIN_ROOT_SYMBOL_NAME;
    const NAME: &'static str = PLUGIN_ROOT_SYMBOL_NAME;
    const VERSION_STRINGS: VersionStrings = abi_stable::package_version_strings!();
}

#[inline]
pub extern "C" fn empty_plugin_ui_assets_v1() -> PluginUiAssetsV1 {
    PluginUiAssetsV1::empty()
}

#[inline]
pub extern "C" fn empty_editor_extensions_v1() -> EditorExtensionsV1 {
    EditorExtensionsV1::empty()
}

/// Defines the exported root module symbol.
#[macro_export]
macro_rules! export_plugin_root {
    ($create:path) => {
        $crate::export_plugin_root!(
            $create,
            $crate::empty_plugin_ui_assets_v1,
            $crate::empty_editor_extensions_v1
        );
    };
    ($create:path, $ui_assets_v1:path) => {
        $crate::export_plugin_root!($create, $ui_assets_v1, $crate::empty_editor_extensions_v1);
    };
    ($create:path, $ui_assets_v1:path, $editor_extensions_v1:path) => {
        #[no_mangle]
        pub extern "C" fn newengine_plugin_root_v1() -> $crate::PluginRootV1Ref {
            <$crate::PluginRootV1 as abi_stable::prefix_type::PrefixTypeTrait>::leak_into_prefix(
                $crate::PluginRootV1 {
                    create: $create,
                    ui_assets_v1: $ui_assets_v1,
                    editor_extensions_v1: $editor_extensions_v1,
                },
            )
        }
    };
}

/// Exports typed capability metadata independently from the V1 module trait.
/// This lets discovery read composition metadata without constructing a plugin.
#[macro_export]
macro_rules! export_plugin_descriptor_v2 {
    ($descriptor:path) => {
        #[no_mangle]
        pub extern "C" fn newengine_plugin_descriptor_v2() -> $crate::PluginDescriptorV2 {
            $descriptor()
        }
    };
}
