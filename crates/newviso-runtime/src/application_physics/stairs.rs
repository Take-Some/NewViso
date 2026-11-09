//! Shared fixed-step stair assistance for upright character capsules.
use super::*;
#[cfg(test)]
mod tests;
use newviso_character::{
    CharacterCollisionWorld, CharacterConfig, StairAdjustment, StairController, SweepHit,
};

#[derive(Debug)]
pub(super) struct CharacterStairs {
    config: CharacterConfig,
    controller: StairController,
    desired: [f32; 3],
    seen: f64,
    support: Option<StairAdjustment>,
    passive_friction: f32,
}
struct World<'a> {
    physics: &'a PhysicsRuntime,
    entity: u64,
    solids: &'a [([f32; 3], [f32; 3])],
}
impl CharacterCollisionWorld for World<'_> {
    fn sweep_sphere(&self, origin: [f32; 3], delta: [f32; 3], radius: f32) -> Option<SweepHit> {
        self.physics
            .character_sweep_sphere(origin, delta, radius, Some(self.entity), self.solids)
    }
    fn support_velocity(&self, entity: u64) -> [f32; 3] {
        self.physics.body_linear_velocity(entity)
    }
}
impl PhysicsRuntime {
    pub(crate) fn register_character_stairs(
        &mut self,
        entity: u64,
        mut config: CharacterConfig,
        desired: [f32; 3],
    ) {
        let Some(body) = self.bodies.get(&entity) else {
            return;
        };
        let CollisionShape::Capsule {
            radius,
            half_height,
        } = body.shape
        else {
            return;
        };
        let passive_friction = body.material.friction;
        config.radius = radius;
        config.half_height = half_height;
        let s = self
            .character_stairs
            .entry(entity)
            .or_insert(CharacterStairs {
                config,
                controller: StairController::default(),
                desired,
                seen: self.fixed_seconds,
                support: None,
                passive_friction,
            });
        if s.config != config {
            s.controller.reset();
        }
        s.config = config;
        s.desired = desired;
        s.seen = self.fixed_seconds;
        // Virtual foot traction owns tangential effort while upright. Letting
        // native capsule friction also grip a riser can turn forward drive into
        // an upward launch. Passive contacts regain their authored material.
        self.bodies.get_mut(&entity).unwrap().material.friction = 0.0;
    }
    pub(crate) fn character_traction_coefficient(&self, entity: u64) -> f32 {
        self.character_stairs.get(&entity).map_or_else(
            || self.surface_friction(Some(entity)),
            |s| s.passive_friction,
        )
    }
    pub(super) fn release_character_stairs(&mut self, entity: u64) {
        if let Some(s) = self.character_stairs.remove(&entity) {
            self.restore_character_stair_material(entity, s.passive_friction);
        }
    }
    fn restore_character_stair_material(&mut self, entity: u64, friction: f32) {
        if let Some(body) = self
            .bodies
            .get_mut(&entity)
            .filter(|b| matches!(b.shape, CollisionShape::Capsule { .. }))
        {
            body.material.friction = friction;
        }
    }
    pub(crate) fn character_contact_skin(&self) -> f32 {
        self.settings.contact_skin
    }
    pub(crate) fn character_has_support(&self, entity: u64) -> bool {
        self.character_stair_support(entity).is_some()
            || self.character_ground_contact_height(entity).is_some()
    }
    /// Generic anatomy can stand without a locomotion intent; collider dimensions
    /// always come from the actual native shape, including after body replacement.
    pub(crate) fn character_foot_config(&self, entity: u64) -> Option<CharacterConfig> {
        if let Some(config) = self.character_stair_config(entity) {
            return Some(config);
        }
        let body = self.bodies.get(&entity)?;
        if body.kind == PhysicsBodyKind::Static
            || body.flags.is_trigger
            || !body.flags.casts_contacts
        {
            return None;
        }
        let (min, max) = self.body_local_bounds(entity)?;
        // The refreshed native bounds have exact primitive support extents
        // (including rotated cylinders), rather than a rotated local AABB.
        let height = (body.position[1] - body.bounds_min[1]).max(0.0);
        let radius = ((max[0] - min[0]) * 0.5)
            .min((max[2] - min[2]) * 0.5)
            .min(height);
        if !radius.is_finite() || radius <= 0.0 {
            return None;
        }
        Some(CharacterConfig {
            radius,
            half_height: (height - radius).max(0.0),
            ..Default::default()
        })
    }
    pub(crate) fn character_stair_config(&self, entity: u64) -> Option<CharacterConfig> {
        self.character_stairs.get(&entity).map(|s| s.config)
    }
    pub(crate) fn character_stair_support(&self, entity: u64) -> Option<StairAdjustment> {
        self.character_stairs.get(&entity)?.support
    }
    pub(super) fn update_character_stairs(
        &mut self,
        dt: f32,
        scene_solids: &[(u64, [f32; 3], [f32; 3])],
    ) -> Result<(), String> {
        if self.character_stairs.is_empty() {
            return Ok(());
        }
        let solids = scene_solids
            .iter()
            .filter(|(id, _, _)| {
                !self.streamed_colliders.contains_key(id) && !self.bodies.contains_key(id)
            })
            .map(|(_, min, max)| (*min, *max))
            .collect::<Vec<_>>();
        let entries = std::mem::take(&mut self.character_stairs);
        for (entity, mut stairs) in entries {
            if self.fixed_seconds - stairs.seen > 0.25 {
                self.restore_character_stair_material(entity, stairs.passive_friction);
                continue;
            }
            let Some(body) = self.bodies.get(&entity) else {
                continue;
            };
            if !matches!(body.shape, CollisionShape::Capsule { .. }) {
                continue;
            }
            let position = body.position;
            let mut velocity = body.linear_velocity;
            let grounded = self.character_ground_contact_height(entity).is_some();
            stairs.support = stairs.controller.update(
                &World {
                    physics: self,
                    entity,
                    solids: &solids,
                },
                stairs.config,
                position,
                velocity,
                stairs.desired,
                grounded,
                dt,
            );
            if let Some(s) = stairs.support {
                velocity[1] = 0.0;
                self.drive_character_body(entity, velocity, Some(s.y))?;
            }
            self.character_stairs.insert(entity, stairs);
        }
        Ok(())
    }
}
