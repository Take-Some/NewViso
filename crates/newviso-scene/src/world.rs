use crate::math::Vec3;
use std::collections::HashMap;

mod lifecycle;
mod mutation;
mod visibility;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct SceneEntityId(pub(crate) u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SceneEntityKind {
    Camera,
    StaticMesh,
    DynamicMesh,
    Light,
    SkyVisual,
    Trigger,
    Portal,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SceneMobility {
    Static,
    Dynamic,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LightType {
    Directional,
    Point,
    Spot,
    Area,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LightComponent {
    pub(crate) light_type: LightType,
    pub(crate) color: [f32; 3],
    pub(crate) intensity: f32,
    pub(crate) range: f32,
    pub(crate) cone_inner_degrees: f32,
    pub(crate) cone_outer_degrees: f32,
    pub(crate) casts_shadows: bool,
    pub(crate) shadow_bias: f32,
    pub(crate) shadow_normal_bias: f32,
    pub(crate) shadow_resolution: u32,
    pub(crate) shadow_distance: f32,
}

impl LightComponent {
    pub(crate) fn validate(self) -> Result<Self, String> {
        if self
            .color
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0)
            || !self.intensity.is_finite()
            || self.intensity < 0.0
            || !self.range.is_finite()
            || self.range < 0.0
            || !self.cone_inner_degrees.is_finite()
            || !self.cone_outer_degrees.is_finite()
            || self.cone_inner_degrees < 0.0
            || self.cone_outer_degrees < self.cone_inner_degrees
            || self.cone_outer_degrees > 179.0
            || !self.shadow_bias.is_finite()
            || self.shadow_bias < 0.0
            || !self.shadow_normal_bias.is_finite()
            || self.shadow_normal_bias < 0.0
            || self.shadow_resolution == 0
            || !self.shadow_distance.is_finite()
            || self.shadow_distance <= 0.0
        {
            return Err("invalid generic LightComponent parameters".to_owned());
        }
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SceneLifecycle {
    Constructed,
    Added,
    Active,
    Dormant,
    PendingRemove,
    Removed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SceneResidency {
    Unloaded,
    Requested,
    Resident,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum VisibilityModule {
    Debug = 0,
    Camera = 1,
    Script = 2,
    Gameplay = 3,
    Frontend = 4,
    Vfx = 5,
    World = 6,
    Player = 7,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VisibilityMask(u32);

impl Default for VisibilityMask {
    fn default() -> Self {
        Self(u32::MAX)
    }
}

impl VisibilityMask {
    pub(crate) fn set(&mut self, module: VisibilityModule, visible: bool) {
        let bit = 1_u32 << module as u8;
        if visible {
            self.0 |= bit;
        } else {
            self.0 &= !bit;
        }
    }

    #[cfg(test)]
    pub(crate) fn visible_for(self, module: VisibilityModule) -> bool {
        self.0 & (1_u32 << module as u8) != 0
    }

    pub(crate) fn all_visible(self) -> bool {
        self.0 == u32::MAX
    }

    pub(crate) fn raw(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct SceneBounds {
    pub(crate) min: Vec3,
    pub(crate) max: Vec3,
}

impl SceneBounds {
    pub(crate) fn from_center_half_extent(center: Vec3, half: Vec3) -> Self {
        Self {
            min: Vec3::new(center.x - half.x, center.y - half.y, center.z - half.z),
            max: Vec3::new(center.x + half.x, center.y + half.y, center.z + half.z),
        }
    }

    pub(crate) fn center(self) -> Vec3 {
        Vec3::new(
            (self.min.x + self.max.x) * 0.5,
            (self.min.y + self.max.y) * 0.5,
            (self.min.z + self.max.z) * 0.5,
        )
    }

    pub(crate) fn radius(self) -> f32 {
        self.max.sub(self.center()).length()
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct SceneLodPolicy {
    pub(crate) visible_distance: f32,
    pub(crate) stream_distance: f32,
    pub(crate) fade_range: f32,
}

impl Default for SceneLodPolicy {
    fn default() -> Self {
        Self {
            visible_distance: f32::INFINITY,
            stream_distance: f32::INFINITY,
            fade_range: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct SceneTransform {
    pub(crate) position: Vec3,
    pub(crate) rotation_degrees: Vec3,
    pub(crate) scale: Vec3,
}

#[derive(Clone, Debug)]
pub(crate) struct SceneEntity {
    pub(crate) id: SceneEntityId,
    pub(crate) name: String,
    pub(crate) kind: SceneEntityKind,
    pub(crate) mobility: SceneMobility,
    pub(crate) lifecycle: SceneLifecycle,
    pub(crate) transform: SceneTransform,
    pub(crate) light: Option<LightComponent>,
    pub(crate) bounds: SceneBounds,
    pub(crate) parent: Option<SceneEntityId>,
    pub(crate) children: Vec<SceneEntityId>,
    pub(crate) visibility: VisibilityMask,
    pub(crate) lod: SceneLodPolicy,
    pub(crate) solid: bool,
    pub(crate) asset_ref: Option<String>,
    pub(crate) render_slot: Option<usize>,
    pub(crate) residency: SceneResidency,
    pub(crate) priority_score: f32,
    pub(crate) lod_alpha: f32,
    pub(crate) last_visible_frame: Option<u64>,
}

impl SceneEntity {
    pub(crate) fn is_renderable(&self) -> bool {
        self.render_slot.is_some()
            && self.lifecycle == SceneLifecycle::Active
            && self.residency == SceneResidency::Resident
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SceneFocusSource {
    Camera,
    Entity(SceneEntityId),
    Override,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct SceneFocus {
    pub(crate) position: Vec3,
    pub(crate) velocity: Vec3,
    pub(crate) source: SceneFocusSource,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct SceneView {
    pub(crate) position: Vec3,
    pub(crate) forward: Vec3,
    pub(crate) up: Vec3,
    pub(crate) near: f32,
    pub(crate) far: f32,
    pub(crate) fov_y_radians: f32,
    pub(crate) aspect: f32,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct SceneFramePlan {
    pub(crate) frame: u64,
    pub(crate) visible_render_slots: Vec<usize>,
    pub(crate) visible_entities: Vec<SceneEntityId>,
    pub(crate) requested_entities: Vec<SceneEntityId>,
    pub(crate) streaming_entities: Vec<SceneEntityId>,
    pub(crate) visible_count: usize,
    pub(crate) culled_count: usize,
    pub(crate) resident_count: usize,
    pub(crate) dynamic_count: usize,
}

#[derive(Debug)]
pub(crate) struct SceneWorld {
    entities: Vec<SceneEntity>,
    by_id: HashMap<SceneEntityId, usize>,
    frame: u64,
    focus: SceneFocus,
    last_focus_position: Vec3,
    last_dt: f32,
}

impl SceneWorld {
    pub(crate) fn new(initial_focus: Vec3) -> Self {
        Self {
            entities: Vec::new(),
            by_id: HashMap::new(),
            frame: 0,
            focus: SceneFocus {
                position: initial_focus,
                velocity: Vec3::ZERO,
                source: SceneFocusSource::Camera,
            },
            last_focus_position: initial_focus,
            last_dt: 0.0,
        }
    }
}

fn stream_priority(radius: f32, distance: f32, mobility: SceneMobility) -> f32 {
    let size_score = radius.max(0.05) / distance.max(0.5);
    let mobility_bias = match mobility {
        SceneMobility::Static => 1.0,
        SceneMobility::Dynamic => 1.35,
    };
    size_score * mobility_bias
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entity(id: u64, z: f32, render_slot: usize) -> SceneEntity {
        SceneEntity {
            id: SceneEntityId(id),
            name: format!("entity-{id}"),
            kind: SceneEntityKind::StaticMesh,
            mobility: SceneMobility::Static,
            lifecycle: SceneLifecycle::Constructed,
            transform: SceneTransform {
                position: Vec3::new(0.0, 0.0, z),
                rotation_degrees: Vec3::ZERO,
                scale: Vec3::ONE,
            },
            light: None,
            bounds: SceneBounds::from_center_half_extent(
                Vec3::new(0.0, 0.0, z),
                Vec3::new(1.0, 1.0, 1.0),
            ),
            parent: None,
            children: Vec::new(),
            visibility: VisibilityMask::default(),
            lod: SceneLodPolicy::default(),
            solid: false,
            asset_ref: None,
            render_slot: Some(render_slot),
            residency: SceneResidency::Resident,
            priority_score: 0.0,
            lod_alpha: 1.0,
            last_visible_frame: None,
        }
    }

    fn view() -> SceneView {
        SceneView {
            position: Vec3::ZERO,
            forward: Vec3::new(0.0, 0.0, -1.0),
            up: Vec3::Y,
            near: 0.1,
            far: 100.0,
            fov_y_radians: 70.0_f32.to_radians(),
            aspect: 16.0 / 9.0,
        }
    }

    #[test]
    fn lifecycle_and_visibility_scan_match_scene_phases() {
        let mut world = SceneWorld::new(Vec3::ZERO);
        world.add_entity(entity(1, -8.0, 0)).unwrap();
        world.add_entity(entity(2, 8.0, 1)).unwrap();
        world.activate_all();
        world.pre_update(Vec3::ZERO, 1.0 / 60.0);
        world.update();

        let plan = world.scan_visibility(view());
        assert_eq!(plan.visible_render_slots, vec![0]);
        assert_eq!(plan.visible_count, 1);
        assert_eq!(plan.culled_count, 1);
    }

    #[test]
    fn visibility_modules_can_hide_entity_without_removing_it() {
        let mut world = SceneWorld::new(Vec3::ZERO);
        world.add_entity(entity(1, -8.0, 0)).unwrap();
        world.activate_all();
        world
            .set_visibility(SceneEntityId(1), VisibilityModule::Script, false)
            .unwrap();
        assert!(!world
            .entity(SceneEntityId(1))
            .unwrap()
            .visibility
            .visible_for(VisibilityModule::Script));
        world
            .set_visibility(SceneEntityId(1), VisibilityModule::Camera, false)
            .unwrap();
        world.pre_update(Vec3::ZERO, 1.0 / 60.0);
        let plan = world.scan_visibility(view());
        assert!(plan.visible_render_slots.is_empty());
    }

    #[test]
    fn generic_light_component_is_stored_without_game_semantics() {
        let mut e = entity(9, -4.0, 0);
        e.kind = SceneEntityKind::Light;
        e.render_slot = None;
        let mut world = SceneWorld::new(Vec3::ZERO);
        world.add_entity(e).unwrap();
        world.activate_all();

        world
            .set_light(
                SceneEntityId(9),
                Some(LightComponent {
                    light_type: LightType::Directional,
                    color: [1.0, 0.9, 0.8],
                    intensity: 1.5,
                    range: 0.0,
                    cone_inner_degrees: 0.0,
                    cone_outer_degrees: 0.0,
                    casts_shadows: true,
                    shadow_bias: 0.0015,
                    shadow_normal_bias: 0.02,
                    shadow_resolution: 2048,
                    shadow_distance: 96.0,
                }),
            )
            .unwrap();

        let lights = world.active_lights();
        assert_eq!(lights.len(), 1);
        assert_eq!(lights[0].2.light_type, LightType::Directional);
        assert!(lights[0].2.casts_shadows);
    }

    #[test]
    fn dynamic_entities_receive_streaming_priority_bias() {
        let static_score = stream_priority(1.0, 10.0, SceneMobility::Static);
        let dynamic_score = stream_priority(1.0, 10.0, SceneMobility::Dynamic);
        assert!(dynamic_score > static_score);
    }

    #[test]
    fn conservative_guard_band_keeps_edge_visible_entity() {
        let mut world = SceneWorld::new(Vec3::ZERO);
        let mut e = entity(1, -10.0, 0);
        e.transform.position = Vec3::new(14.5, 0.0, -10.0);
        e.bounds =
            SceneBounds::from_center_half_extent(e.transform.position, Vec3::new(1.0, 1.0, 1.0));
        world.add_entity(e).unwrap();
        world.activate_all();
        world.pre_update(Vec3::ZERO, 1.0 / 60.0);

        let plan = world.scan_visibility(view());
        assert_eq!(plan.visible_render_slots, vec![0]);
    }

    #[test]
    fn hierarchy_propagates_visibility_and_rejects_cycles() {
        let mut world = SceneWorld::new(Vec3::ZERO);
        world.add_entity(entity(1, -8.0, 0)).unwrap();
        world.add_entity(entity(2, -8.0, 1)).unwrap();
        world.activate_all();
        world
            .set_parent(SceneEntityId(2), Some(SceneEntityId(1)))
            .unwrap();
        assert!(world
            .set_parent(SceneEntityId(1), Some(SceneEntityId(2)))
            .is_err());

        world
            .set_visibility(SceneEntityId(1), VisibilityModule::World, false)
            .unwrap();
        world.pre_update(Vec3::ZERO, 1.0 / 60.0);
        let plan = world.scan_visibility(view());
        assert!(!plan.visible_entities.contains(&SceneEntityId(2)));
    }

    #[test]
    fn focus_tracks_entity_and_supports_explicit_override() {
        let mut world = SceneWorld::new(Vec3::ZERO);
        world.add_entity(entity(7, -12.0, 0)).unwrap();
        world.activate_all();

        world.set_focus_entity(SceneEntityId(7)).unwrap();
        world.pre_update(Vec3::new(99.0, 0.0, 0.0), 1.0 / 60.0);
        assert!((world.focus().position.z + 12.0).abs() < 0.001);

        world.set_focus_override(Vec3::new(3.0, 4.0, 5.0), Vec3::new(1.0, 0.0, 0.0));
        world.pre_update(Vec3::ZERO, 1.0 / 60.0);
        assert_eq!(world.focus().source, SceneFocusSource::Override);
        assert!((world.focus().position.x - 3.0).abs() < 0.001);

        world.set_focus_camera();
        world.pre_update(Vec3::new(2.0, 0.0, 0.0), 1.0 / 60.0);
        assert_eq!(world.focus().source, SceneFocusSource::Camera);
        assert!((world.focus().position.x - 2.0).abs() < 0.001);
    }

    #[test]
    fn unloaded_assets_are_requested_in_priority_order() {
        let mut world = SceneWorld::new(Vec3::ZERO);
        let mut far = entity(1, -30.0, 0);
        far.residency = SceneResidency::Unloaded;
        far.asset_ref = Some("assets/far_model@main".to_owned());
        let mut near = entity(2, -5.0, 1);
        near.residency = SceneResidency::Unloaded;
        near.asset_ref = Some("assets/near_model@main".to_owned());
        world.add_entity(far).unwrap();
        world.add_entity(near).unwrap();
        world.activate_all();
        world.pre_update(Vec3::ZERO, 1.0 / 60.0);

        let plan = world.scan_visibility(view());
        assert_eq!(
            plan.requested_entities,
            vec![SceneEntityId(2), SceneEntityId(1)]
        );
        assert_eq!(
            plan.streaming_entities,
            vec![SceneEntityId(2), SceneEntityId(1)]
        );

        world
            .set_residency(SceneEntityId(2), SceneResidency::Resident)
            .unwrap();
        let next = world.scan_visibility(view());
        assert_eq!(
            next.streaming_entities,
            vec![SceneEntityId(2), SceneEntityId(1)]
        );
        assert!(!next.requested_entities.contains(&SceneEntityId(2)));
    }

    #[test]
    fn lod_fade_is_computed_before_visibility_gather() {
        let mut world = SceneWorld::new(Vec3::ZERO);
        let mut e = entity(1, -8.0, 0);
        e.lod = SceneLodPolicy {
            visible_distance: 10.0,
            stream_distance: 12.0,
            fade_range: 4.0,
        };
        world.add_entity(e).unwrap();
        world.activate_all();
        world.pre_update(Vec3::ZERO, 1.0 / 60.0);

        let plan = world.scan_visibility(view());
        assert_eq!(plan.visible_count, 1);
        let alpha = world.entity(SceneEntityId(1)).unwrap().lod_alpha;
        assert!(alpha > 0.0 && alpha < 1.0);
    }
}
