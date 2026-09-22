use abi_stable::std_types::RString;

pub const SIGNATURE_SYMBOL: &[u8] = b"newengine_plugin_signature_v1\0";
pub const ROOT_SYMBOL: &[u8] = b"newengine_plugin_root_v1\0";
pub const LEGACY_ROOT_SYMBOL: &[u8] = b"export_plugin_root\0";
pub const DESCRIPTOR_V2_SYMBOL: &[u8] = b"newengine_plugin_descriptor_v2\0";

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderKind {
    Runtime = 1,
    Importer = 2,
    Editor = 3,
    Tool = 4,
    Other = 255,
}

impl ProviderKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Runtime => "runtime",
            Self::Importer => "importer",
            Self::Editor => "editor",
            Self::Tool => "tool",
            Self::Other => "other",
        }
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BootstrapPhase {
    Bootstrap = 1,
    Platform = 2,
    Engine = 3,
}

impl BootstrapPhase {
    pub fn label(self) -> &'static str {
        match self {
            Self::Bootstrap => "bootstrap",
            Self::Platform => "platform",
            Self::Engine => "engine",
        }
    }
}

#[repr(C)]
pub struct ProviderSignatureV1 {
    pub id: RString,
    pub name: RString,
    pub version: RString,
    pub kind: ProviderKind,
    pub bootstrap_phase: BootstrapPhase,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RootSymbol {
    Current,
    Legacy,
    Missing,
}

impl RootSymbol {
    pub fn label(self) -> &'static str {
        match self {
            Self::Current => "newengine_plugin_root_v1",
            Self::Legacy => "export_plugin_root",
            Self::Missing => "<missing>",
        }
    }
}
