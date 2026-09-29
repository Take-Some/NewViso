use serde::{Deserialize, Serialize};

use crate::UiDrawList;

/// Stable retained-UI presentation domains shared by UI composition and rendering.
///
/// This low-level identity intentionally lives beside `UiDrawList`: both UI runtime and
/// render protocol may depend on it without creating a `ui-api <-> render-api` cycle.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum UiLayerDomain {
    /// HUD/overlay/menu/modal surfaces attached to a playable viewport.
    GameViewport,
    /// Editor chrome, panels and tooling surfaces.
    Editor,
    /// Bootstrap/loading/front-end/error surfaces owned by the engine shell.
    #[default]
    System,
    /// Diagnostics/profiler/debug overlays. Always composed last by default.
    Debug,
}

impl UiLayerDomain {
    #[inline]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::GameViewport => "game_viewport",
            Self::Editor => "editor",
            Self::System => "system",
            Self::Debug => "debug",
        }
    }

    /// Default cross-domain composition order. Authored z-order remains local to a
    /// domain; this value orders complete retained domain packets at the renderer boundary.
    #[inline]
    pub const fn default_composition_order(self) -> i32 {
        match self {
            Self::GameViewport => 100,
            Self::Editor => 300,
            Self::System => 600,
            Self::Debug => 1_000,
        }
    }
}

/// One renderer-consumable retained UI domain packet.
///
/// The packet contains a logical target and the already-built provider draw stream. It never
/// contains backend textures/framebuffers/descriptor handles; those remain renderer-owned.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UiLayerDrawPacket {
    pub version: u32,
    pub frame_index: u64,
    pub domain: UiLayerDomain,
    pub composition_order: i32,
    pub target_surface_id: String,
    pub surface_ids: Vec<String>,
    pub invalidation_revision: u64,
    pub draw_list: UiDrawList,
}

impl Default for UiLayerDrawPacket {
    fn default() -> Self {
        Self {
            version: 1,
            frame_index: 0,
            domain: UiLayerDomain::System,
            composition_order: UiLayerDomain::System.default_composition_order(),
            target_surface_id: String::new(),
            surface_ids: Vec::new(),
            invalidation_revision: 0,
            draw_list: UiDrawList::new(),
        }
    }
}

impl UiLayerDrawPacket {
    #[inline]
    pub fn new(domain: UiLayerDomain, frame_index: u64, draw_list: UiDrawList) -> Self {
        Self {
            version: 1,
            frame_index,
            domain,
            composition_order: domain.default_composition_order(),
            draw_list,
            ..Self::default()
        }
    }

    #[inline]
    pub fn with_target(mut self, target_surface_id: impl Into<String>) -> Self {
        self.target_surface_id = target_surface_id.into();
        self
    }

    #[inline]
    pub fn with_surfaces(mut self, surface_ids: impl IntoIterator<Item = String>) -> Self {
        self.surface_ids = surface_ids.into_iter().collect();
        self
    }

    #[inline]
    pub fn with_invalidation_revision(mut self, revision: u64) -> Self {
        self.invalidation_revision = revision;
        self
    }
}

/// Ordered packet set crossing the host/runtime -> renderer boundary.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct UiLayerDrawPacketSet {
    pub version: u32,
    pub frame_index: u64,
    pub packets: Vec<UiLayerDrawPacket>,
}

impl UiLayerDrawPacketSet {
    #[inline]
    pub fn new(frame_index: u64) -> Self {
        Self {
            version: 1,
            frame_index,
            packets: Vec::new(),
        }
    }

    pub fn push(&mut self, mut packet: UiLayerDrawPacket) {
        packet.frame_index = self.frame_index;
        if let Some(existing) = self
            .packets
            .iter_mut()
            .find(|it| it.domain == packet.domain)
        {
            *existing = packet;
        } else {
            self.packets.push(packet);
        }
        self.sort_for_composite();
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.packets.is_empty()
    }

    #[inline]
    pub fn draw_list(&self, domain: UiLayerDomain) -> Option<&UiDrawList> {
        self.packets
            .iter()
            .find(|packet| packet.domain == domain)
            .map(|packet| &packet.draw_list)
    }

    #[inline]
    pub fn draw_list_mut(&mut self, domain: UiLayerDomain) -> Option<&mut UiDrawList> {
        self.packets
            .iter_mut()
            .find(|packet| packet.domain == domain)
            .map(|packet| &mut packet.draw_list)
    }

    #[inline]
    pub fn replace_draw_list(&mut self, domain: UiLayerDomain, draw_list: UiDrawList) -> bool {
        let Some(packet) = self
            .packets
            .iter_mut()
            .find(|packet| packet.domain == domain)
        else {
            return false;
        };
        packet.draw_list = draw_list;
        true
    }

    #[inline]
    pub fn first_draw_list_mut(&mut self) -> Option<&mut UiDrawList> {
        self.packets.first_mut().map(|packet| &mut packet.draw_list)
    }

    pub fn sort_for_composite(&mut self) {
        self.packets.sort_by(|a, b| {
            (a.composition_order, a.domain, a.target_surface_id.as_str()).cmp(&(
                b.composition_order,
                b.domain,
                b.target_surface_id.as_str(),
            ))
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packet_set_composes_game_editor_system_debug_in_stable_order() {
        let mut set = UiLayerDrawPacketSet::new(7);
        for domain in [
            UiLayerDomain::Debug,
            UiLayerDomain::System,
            UiLayerDomain::GameViewport,
            UiLayerDomain::Editor,
        ] {
            set.push(UiLayerDrawPacket::new(domain, 0, UiDrawList::new()));
        }
        assert_eq!(
            set.packets
                .iter()
                .map(|packet| packet.domain)
                .collect::<Vec<_>>(),
            vec![
                UiLayerDomain::GameViewport,
                UiLayerDomain::Editor,
                UiLayerDomain::System,
                UiLayerDomain::Debug,
            ]
        );
        assert!(set.packets.iter().all(|packet| packet.frame_index == 7));
    }

    #[test]
    fn packet_set_replaces_same_domain_instead_of_double_compositing_it() {
        let mut set = UiLayerDrawPacketSet::new(3);
        set.push(
            UiLayerDrawPacket::new(UiLayerDomain::GameViewport, 3, UiDrawList::new())
                .with_invalidation_revision(1),
        );
        set.push(
            UiLayerDrawPacket::new(UiLayerDomain::GameViewport, 3, UiDrawList::new())
                .with_invalidation_revision(2),
        );
        assert_eq!(set.packets.len(), 1);
        assert_eq!(set.packets[0].invalidation_revision, 2);
    }
}
