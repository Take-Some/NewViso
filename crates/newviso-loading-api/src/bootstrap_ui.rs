#![forbid(unsafe_op_in_unsafe_fn)]

/// Shared bootstrap data palette for engine.ui projections. This module does not
/// render UI and must not be consumed by a out-of-band compositor; it only
/// keeps brand/status data stable for providers that implement `engine.ui`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BootstrapUiRgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl BootstrapUiRgb {
    #[inline]
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BootstrapUiPalette {
    pub bg: BootstrapUiRgb,
    pub bg_deep: BootstrapUiRgb,
    pub panel: BootstrapUiRgb,
    pub panel_active: BootstrapUiRgb,
    pub edge: BootstrapUiRgb,
    pub edge_soft: BootstrapUiRgb,
    pub text: BootstrapUiRgb,
    pub text_dim: BootstrapUiRgb,
    pub muted: BootstrapUiRgb,
    pub blue: BootstrapUiRgb,
    pub blue_bright: BootstrapUiRgb,
    pub silver: BootstrapUiRgb,
    pub ok: BootstrapUiRgb,
    pub warn: BootstrapUiRgb,
    pub fail: BootstrapUiRgb,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BootstrapUiAssetDictionary {
    pub logical_path: &'static str,
    pub background_entry: &'static str,
    pub logo_entry: &'static str,
    pub spinner_entry: &'static str,
    pub symbol_entry: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BootstrapUiBrand {
    pub product: &'static str,
    pub prestart_title: &'static str,
    pub prestart_subtitle: &'static str,
    pub preload_title: &'static str,
    pub preload_status: &'static str,
    pub tagline: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BootstrapUiStepSpec {
    pub id: &'static str,
    pub label: &'static str,
    pub detail: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BootstrapUiStyle {
    pub brand: BootstrapUiBrand,
    pub palette: BootstrapUiPalette,
    pub assets: BootstrapUiAssetDictionary,
    pub steps: &'static [BootstrapUiStepSpec],
}

pub const NORTH_STAR_BOOTSTRAP_UI_STEPS: &[BootstrapUiStepSpec] = &[
    BootstrapUiStepSpec {
        id: "platform",
        label: "Platform",
        detail: "Native compositor is rendering independently",
    },
    BootstrapUiStepSpec {
        id: "service_call",
        label: "Service Call",
        detail: "Gateway request completed",
    },
    BootstrapUiStepSpec {
        id: "task_ctrl",
        label: "Task Control",
        detail: "Cooperative startup task state",
    },
    BootstrapUiStepSpec {
        id: "event_bus",
        label: "Event Bus",
        detail: "Runtime event stream",
    },
    BootstrapUiStepSpec {
        id: "renderer",
        label: "Preparing Renderer",
        detail: "Compiling shaders",
    },
    BootstrapUiStepSpec {
        id: "assets",
        label: "Building Asset Registry",
        detail: "Gathering asset metadata",
    },
    BootstrapUiStepSpec {
        id: "handoff",
        label: "Finalizing Runtime Handoff",
        detail: "Completing startup",
    },
];

pub const NORTH_STAR_BOOTSTRAP_UI_STYLE: BootstrapUiStyle = BootstrapUiStyle {
    brand: BootstrapUiBrand {
        product: "North Star Engine",
        prestart_title: "North Star Engine",
        prestart_subtitle: "Display, graphics and launch configuration",
        preload_title: "NORTH STAR ENGINE",
        preload_status: "LOADING ENGINE",
        tagline: "BUILT FOR CREATORS. ENGINEERED FOR WORLDS.",
    },
    palette: BootstrapUiPalette {
        bg: BootstrapUiRgb::new(3, 7, 14),
        bg_deep: BootstrapUiRgb::new(0, 3, 8),
        panel: BootstrapUiRgb::new(10, 17, 28),
        panel_active: BootstrapUiRgb::new(20, 45, 84),
        edge: BootstrapUiRgb::new(42, 58, 82),
        edge_soft: BootstrapUiRgb::new(25, 36, 54),
        text: BootstrapUiRgb::new(236, 243, 255),
        text_dim: BootstrapUiRgb::new(171, 184, 206),
        muted: BootstrapUiRgb::new(116, 131, 156),
        blue: BootstrapUiRgb::new(70, 151, 255),
        blue_bright: BootstrapUiRgb::new(112, 192, 255),
        silver: BootstrapUiRgb::new(207, 218, 235),
        ok: BootstrapUiRgb::new(98, 192, 255),
        warn: BootstrapUiRgb::new(255, 184, 112),
        fail: BootstrapUiRgb::new(255, 92, 108),
    },
    assets: BootstrapUiAssetDictionary {
        logical_path: "textures/ui/loading/loaderWindow.ytd",
        background_entry: "north_star_preload_background",
        logo_entry: "north_star_engine_logo",
        spinner_entry: "north_star_loading_spinner",
        symbol_entry: "north_star_engine_symbol",
    },
    steps: NORTH_STAR_BOOTSTRAP_UI_STEPS,
};

#[inline]
pub const fn north_star_bootstrap_ui_style() -> BootstrapUiStyle {
    NORTH_STAR_BOOTSTRAP_UI_STYLE
}
