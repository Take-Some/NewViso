//! Independent lower-limb channel using the length-preserving CPU/GPU solver.
use super::*;
#[derive(Clone, Debug)]
pub struct SceneFootSupport {
    pub surface_id: String,
    pub normal: [f32; 3],
    pub authored_ankle: [f32; 3],
    pub target_sole: [[f32; 3]; 4],
    /// Metre offsets in the ankle's rotation frame (scale already included).
    pub local_sole: [[f32; 3]; 4],
    pub planted: bool,
    pub contact_weight: f32,
    pub contact_point: Option<[f32; 3]>,
    pub contact_gap: Option<f32>,
    pub phase_weight: f32,
    pub landing_weight: f32,
    pub contact_demand: f32,
    pub contact_sole: [[f32; 3]; 4],
    pub contact_target: [f32; 3],
    pub authored_rotation: [f32; 4],
    pub correction_velocity: [f32; 3],
    pub constraint_response: [f32; 3],
    pub held: bool,
    pub anchored_goal: bool,
    pub preview_goal: bool,
    pub surface_complete: bool,
    pub has_surface: bool,
    pub collision_adjustment: f32,
    pub orientation_adjustment: f32,
    pub horizontal_adjustment: f32,
    pub reach_projected: bool,
    pub root_offset: f32,
}
#[derive(Clone, Debug)]
pub struct SceneLegIkConstraint {
    pub hip_joint: String,
    pub knee_joint: String,
    pub foot_joint: String,
    pub target: [f32; 3],
    pub pole: [f32; 3],
    pub rotation_degrees: [f32; 3],
    pub weight: f32,
    pub support: Option<SceneFootSupport>,
}
impl SceneLegIkConstraint {
    pub(super) fn solver_constraint(&self) -> SceneArmIkConstraint {
        SceneArmIkConstraint {
            upper_joint: self.hip_joint.clone(),
            lower_joint: self.knee_joint.clone(),
            hand_joint: self.foot_joint.clone(),
            tip_joint: self.foot_joint.clone(),
            target: self.target,
            pole: self.pole,
            rotation_degrees: self.rotation_degrees,
            weight: self.weight,
            hand_pose_weight: 0.0,
            joint_rotations: Vec::new(),
        }
    }
}
fn validate_leg_constraints(solver: &[SceneArmIkConstraint]) -> Result<(), String> {
    if solver.len() > newviso_character::MAX_SUPPORT_LIMBS {
        return Err("too many supporting limbs".into());
    }
    let mut driven = BTreeSet::new();
    for c in solver {
        arm_ik::layers::validate_arm_constraints(std::slice::from_ref(c))?;
        for joint in [&c.upper_joint, &c.lower_joint, &c.hand_joint] {
            if !driven.insert(joint.to_ascii_lowercase()) {
                return Err("supporting limb chains overlap".into());
            }
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests;
impl Scene3dRuntime {
    pub fn set_entity_leg_ik(
        &mut self,
        id: u64,
        legs: Vec<SceneLegIkConstraint>,
    ) -> Result<(), String> {
        if self.world.entity(SceneEntityId(id)).is_none() {
            return Err("leg IK entity disappeared".into());
        }
        let solver = legs
            .iter()
            .map(SceneLegIkConstraint::solver_constraint)
            .collect::<Vec<_>>();
        validate_leg_constraints(&solver)?;
        if legs.is_empty() {
            self.leg_ik_constraints.remove(&id);
        } else {
            self.leg_ik_constraints.insert(id, legs);
        }
        Ok(())
    }
    pub fn entity_leg_ik_snapshot(&self) -> serde_json::Value {
        serde_json::json!(self
            .leg_ik_constraints
            .iter()
            .map(|(id, legs)| {
                let names = legs
                    .iter()
                    .flat_map(|c| {
                        [
                            c.hip_joint.as_str(),
                            c.knee_joint.as_str(),
                            c.foot_joint.as_str(),
                        ]
                    })
                    .collect::<Vec<_>>();
                let matrices = self.entity_joint_world_matrices(*id, &names).ok();
                let matrix = |name: &str| matrices.as_ref().and_then(|m| m.get(name));
                let point = |name: &str| matrix(name).map(|m| [m[12], m[13], m[14]]);
                serde_json::json!({"entity_key":id.to_string(),"legs":legs.iter().map(|c|{
    let support=c.support.as_ref().map(|s|{
     let actual=matrix(&c.foot_joint).map(|m|{
      let r=trs_matrix([0.0;3],arm_ik::rotation_quat(*m),[1.0;3]);
      s.local_sole.map(|p|{
       let p=transform_vector_mat4(r,Vec3::new(p[0],p[1],p[2]));
       [p.x+m[12],p.y+m[13],p.z+m[14]]
      })
     });
     serde_json::json!({"surface_id":s.surface_id,"normal":s.normal,
      "authored_ankle":s.authored_ankle,"target_sole":s.target_sole,
      "actual_sole":actual,"planted":s.planted,"root_offset":s.root_offset,
      "contact_weight":s.contact_weight,"contact_point":s.contact_point,"contact_gap":s.contact_gap,"phase_weight":s.phase_weight,"landing_weight":s.landing_weight,"contact_demand":s.contact_demand,"contact_sole":s.contact_sole,
      "contact_target":s.contact_target,"authored_rotation":s.authored_rotation,
      "actual_rotation":matrix(&c.foot_joint).map(|m|arm_ik::rotation_quat(*m)),
      "correction_velocity":s.correction_velocity,"constraint_response":s.constraint_response,"held":s.held,"anchored_goal":s.anchored_goal,"preview_goal":s.preview_goal,
      "surface_complete":s.surface_complete,"has_surface":s.has_surface,"collision_adjustment":s.collision_adjustment,
      "orientation_adjustment":s.orientation_adjustment,"horizontal_adjustment":s.horizontal_adjustment,"reach_projected":s.reach_projected})
    });
    serde_json::json!({"foot":c.foot_joint,"target":c.target,
     "hip":point(&c.hip_joint),"knee":point(&c.knee_joint),
     "position":point(&c.foot_joint),"weight":c.weight,"support":support})
   }).collect::<Vec<_>>()})
            })
            .collect::<Vec<_>>())
    }
}
