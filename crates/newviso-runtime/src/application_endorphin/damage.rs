use super::*;
use newviso_endorphin::math;

impl EngineApplication {
    pub(crate) fn ped_body_part_alias(&self, id: &str, name: &str) -> Option<String> {
        let body = self.ped_physics.bodies.get(id)?;
        let anatomy = body.endorphin.model.anatomy.as_ref()?;
        anatomy.part_for_alias(name).map(str::to_owned).or_else(|| {
            body.endorphin
                .model
                .rig
                .parts
                .iter()
                .find(|p| p.joint == name)
                .map(|p| p.name.clone())
        })
    }
    pub(crate) fn ped_body_damage_multiplier(&self, id: &str, name: &str) -> Option<f32> {
        let anatomy = self
            .ped_physics
            .bodies
            .get(id)?
            .endorphin
            .model
            .anatomy
            .as_ref()?;
        anatomy.damage_multiplier(name).or_else(|| {
            anatomy
                .parts
                .values()
                .find(|p| p.region == name)
                .map(|p| p.damage_multiplier)
        })
    }
    /// Physical limb identity is authoritative. Upright receivers use the same
    /// authored bone segments/radii, rather than a world-height classifier.
    pub(crate) fn ped_body_contact(
        &self,
        id: &str,
        entity: u64,
        point: [f32; 3],
    ) -> Option<(String, String, f32)> {
        let body = self.ped_physics.bodies.get(id)?;
        let anatomy = body.endorphin.model.anatomy.as_ref()?;
        if let Some(rig) = &body.endorphin.ragdoll {
            if let Some(part) = rig.parts.iter().find(|p| p.entity == entity) {
                let data = anatomy.parts.get(&part.name)?;
                return Some((
                    part.name.clone(),
                    data.region.clone(),
                    data.damage_multiplier,
                ));
            }
        }
        body.endorphin
            .model
            .rig
            .parts
            .iter()
            .filter_map(|part| {
                let a = self
                    .scene
                    .entity_joint_world_position(body.entity, &part.joint)
                    .ok()?;
                let b = part
                    .end_joint
                    .as_ref()
                    .and_then(|j| self.scene.entity_joint_world_position(body.entity, j).ok())
                    .unwrap_or(a);
                let ab = math::sub(b, a);
                let square = math::dot(ab, ab);
                let t = if square > f32::EPSILON {
                    (math::dot(math::sub(point, a), ab) / square).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let closest = math::add(a, ab.map(|v| v * t));
                let distance = math::length(math::sub(point, closest)) / part.radius;
                let data = anatomy.parts.get(&part.name)?;
                Some((distance, part.name.as_str(), data))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, name, data)| (name.to_owned(), data.region.clone(), data.damage_multiplier))
    }
    pub(crate) fn ped_body_grip_impact(
        &self,
        id: &str,
        part: &str,
    ) -> newviso_agent::peds::grip::GripImpact {
        use newviso_agent::peds::grip::GripImpact;
        let Some(anatomy) = self
            .ped_physics
            .bodies
            .get(id)
            .and_then(|b| b.endorphin.model.anatomy.as_ref())
        else {
            return GripImpact::Body;
        };
        for (function, impact) in [
            ("left_grip", GripImpact::LeftArm),
            ("right_grip", GripImpact::RightArm),
        ] {
            if anatomy
                .functions
                .get(function)
                .is_some_and(|members| members.iter().any(|p| p == part))
            {
                return impact;
            }
        }
        GripImpact::Body
    }
    pub(crate) fn record_ped_body_damage(
        &mut self,
        id: &str,
        part: Option<&str>,
        before_health: f32,
    ) -> Result<(), String> {
        let Some(part) = part else {
            return Ok(());
        };
        let Some(ped) = self.peds.state(id) else {
            return Ok(());
        };
        let fraction = (before_health - ped.health).max(0.0) / ped.profile.max_health;
        let Some(body) = self.ped_physics.bodies.get_mut(id) else {
            return Ok(());
        };
        let Some(model) = body.endorphin.model.anatomy.as_ref() else {
            return Ok(());
        };
        if body.endorphin.damage.injure(model, part, fraction) {
            host::publish_event_json(
                "ped.body.injured",
                "newviso.endorphin",
                json!({"actor_id":id,"part":part,"lost_vitality_fraction":fraction,
                    "capacity":body.endorphin.damage.part_capacity(part)}),
            )?;
        }
        Ok(())
    }
}
