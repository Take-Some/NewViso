//! Live support transport during an admitted physical-to-animation recovery.
use super::animation::ReactionPhase;
use super::*;
use newviso_endorphin::{math, RecoverySupport, RecoverySupportSample};

#[derive(Clone, Debug, serde::Serialize)]
pub(crate) struct GetupSupportStatus {
    pub status: &'static str,
    pub entity_key: Option<String>,
    pub initial: Option<RecoverySupportSample>,
    pub current: Option<RecoverySupportSample>,
    pub transport_updates: u64,
}
impl Default for GetupSupportStatus {
    fn default() -> Self {
        Self {
            status: "not_requested",
            entity_key: None,
            initial: None,
            current: None,
            transport_updates: 0,
        }
    }
}
impl EngineApplication {
    pub(super) fn ped_physical_getup_active(&self, id: &str) -> bool {
        self.ped_physics.bodies.get(id).is_some_and(|b| {
            !b.reset_requested
                && !b.was_dead
                && !self.peds.is_dead(id)
                && b.reaction.as_ref().is_some_and(|r| {
                    r.phase == ReactionPhase::GetUp && r.cause == "physical_recovery"
                })
        })
    }
    /// Observe geometry even when a driven capsule briefly has no contact manifold.
    /// A world anchor is usable only while its admitted floor is still present.
    pub(super) fn ped_getup_observed_ground(&self, id: &str) -> Option<f32> {
        if !self.ped_physical_getup_active(id) {
            return None;
        }
        let b = &self.ped_physics.bodies[id];
        if b.endorphin.recovery_support.is_some() {
            return self.ped_getup_support_sample(id).map(|s| s.ground_point[1]);
        }
        let p = b.endorphin.getup_match.placement.as_ref()?.selected?;
        let physics = self.physics.as_ref()?;
        let skin = physics.contact_skin();
        let center = p.capsule_center;
        let min = [
            center[0] - b.radius,
            p.ground - b.radius,
            center[2] - b.radius,
        ];
        let max = [
            center[0] + b.radius,
            p.ground + b.radius + skin,
            center[2] + b.radius,
        ];
        let solids = self.scene.physics_static_solid_aabbs_near(&[(min, max)]);
        let ground = physics.endorphin_getup_ground(
            [center[0], p.ground + b.radius + skin, center[2]],
            2.0 * b.radius + skin,
            &self.ped_getup_query_ignore(id),
            &solids,
            b.endorphin.model.recovery.minimum_support_cosine,
        )?;
        let tolerance = (skin * 4.0).max(32.0 * f32::EPSILON * (p.ground.abs() + 1.0));
        ((ground - p.ground).abs() <= tolerance).then_some(ground)
    }
    pub(crate) fn ped_getup_attachment(&self, id: &str) -> Option<&RecoverySupport> {
        let b = self.ped_physics.bodies.get(id)?;
        if b.reset_requested || b.was_dead || self.peds.is_dead(id) {
            return None;
        }
        let reaction = b.reaction.as_ref()?;
        if reaction.phase != ReactionPhase::GetUp || reaction.cause != "physical_recovery" {
            return None;
        }
        b.endorphin.recovery_support.as_ref()
    }
    pub(crate) fn ped_getup_support_sample(&self, id: &str) -> Option<RecoverySupportSample> {
        let a = self.ped_getup_attachment(id)?;
        a.sample(
            self.physics
                .as_ref()?
                .endorphin_recovery_support_frame(a.entity),
            self.ped_physics.bodies[id]
                .endorphin
                .model
                .recovery
                .minimum_support_cosine,
        )
    }
    fn lose_ped_getup_support(&mut self, id: &str, reason: &'static str) -> Result<bool, String> {
        let b = self.ped_physics.bodies.get_mut(id).expect("recovering ped");
        b.endorphin.getup_support.status = reason;
        b.endorphin.recovery_support = None;
        // Keep the mixed pose until articulation captures it.
        self.start_ped_fall_animation(
            id,
            [0.0; 3],
            if reason == "blocked" {
                "recovery_support_blocked"
            } else {
                "recovery_support_lost"
            },
        );
        self.enter_ped_physical_fall(id, [0.0; 3])?;
        Ok(false)
    }
    pub(crate) fn advance_ped_recovery_support(&mut self, id: &str) -> Result<bool, String> {
        let Some(attachment) = self.ped_getup_attachment(id).cloned() else {
            if self.ped_physical_getup_active(id) && self.ped_getup_observed_ground(id).is_none() {
                return self.lose_ped_getup_support(id, "surface_lost");
            }
            return Ok(true);
        };
        let Some(sample) = self.ped_getup_support_sample(id) else {
            return self.lose_ped_getup_support(id, "lost");
        };
        let body = &self.ped_physics.bodies[id];
        let Some(previous) = body.endorphin.getup_support.current else {
            return self.lose_ped_getup_support(id, "lost");
        };
        let physics = self.physics.as_ref().expect("ped physics");
        let skin = physics.contact_skin();
        let height = 2.0 * (body.radius + body.half_height);
        let min = [
            previous.ground_point[0].min(sample.ground_point[0]) - body.radius - skin,
            previous.ground_point[1].min(sample.ground_point[1]) - body.radius,
            previous.ground_point[2].min(sample.ground_point[2]) - body.radius - skin,
        ];
        let max = [
            previous.ground_point[0].max(sample.ground_point[0]) + body.radius + skin,
            previous.ground_point[1].max(sample.ground_point[1]) + height + skin,
            previous.ground_point[2].max(sample.ground_point[2]) + body.radius + skin,
        ];
        let solids = self.scene.physics_static_solid_aabbs_near(&[(min, max)]);
        let mut ignore = self.ped_getup_query_ignore(id);
        let floor = physics.endorphin_getup_surface(
            [
                sample.ground_point[0],
                sample.ground_point[1] + body.radius + skin,
                sample.ground_point[2],
            ],
            2.0 * body.radius + skin,
            &ignore,
            &solids,
            body.endorphin.model.recovery.minimum_support_cosine,
        );
        let tolerance =
            (skin * 4.0).max(32.0 * f32::EPSILON * (sample.ground_point[1].abs() + 1.0));
        if !floor.is_some_and(|g| {
            g.support
                .as_ref()
                .is_some_and(|s| s.entity == attachment.entity)
                && (g.position[1] - sample.ground_point[1]).abs() <= tolerance
        }) {
            return self.lose_ped_getup_support(id, "surface_lost");
        }
        ignore.push(attachment.entity);
        if !physics.endorphin_getup_carry_clear(
            previous.ground_point,
            sample.ground_point,
            body.radius,
            body.half_height,
            &ignore,
            &solids,
        ) {
            return self.lose_ped_getup_support(id, "blocked");
        }
        let Some(pose) = physics.vehicle_body_state(body.entity) else {
            return self.lose_ped_getup_support(id, "lost");
        };
        let delta = math::sub(sample.ground_point, previous.ground_point);
        let yaw = (sample.heading_radians - previous.heading_radians) * 0.5;
        let rotation = math::multiply([0.0, yaw.sin(), 0.0, yaw.cos()], pose.rotation);
        let position = math::add(pose.position, delta);
        let entity = body.entity;
        self.physics
            .as_mut()
            .expect("ped physics")
            .drive_character_body_pose(entity, position, rotation, sample.velocity)?;
        let body = self.ped_physics.bodies.get_mut(id).expect("recovering ped");
        body.death_heading = sample.heading_radians.to_degrees().rem_euclid(360.0);
        body.endorphin.getup_support.status = "bound";
        body.endorphin.getup_support.current = Some(sample);
        if math::length(delta) > f32::EPSILON
            || (sample.heading_radians - previous.heading_radians)
                .sin()
                .abs()
                > f32::EPSILON
        {
            body.endorphin.getup_support.transport_updates += 1;
        }
        Ok(true)
    }
}
