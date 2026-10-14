//! Joint queries and entity attachments; no worker scheduling.
use super::*;

impl Scene3dRuntime {
    pub(super) fn animated_joint_model_matrix(
        &self,
        stable_id: u64,
        joint_name: &str,
    ) -> Result<[f32; 16], String> {
        let state = self
            .skinned_entities
            .get(&stable_id)
            .ok_or_else(|| format!("scene entity {stable_id} is not an installed skinned model"))?;
        let index = state
            .skeleton
            .joints
            .iter()
            .position(|j| j.name.eq_ignore_ascii_case(joint_name))
            .ok_or_else(|| format!("scene entity {stable_id} has no joint {joint_name}"))?;
        Ok(self.entity_pose_model_matrices(stable_id)?[index])
    }

    pub(in crate::first_scene) fn bind_joint_model_matrix(
        &self,
        stable_id: u64,
        joint_name: &str,
    ) -> Result<[f32; 16], String> {
        let (skeleton, source_to_model) = self
            .entity_skeletons
            .get(&stable_id)
            .ok_or_else(|| format!("scene entity {stable_id} has no installed model skeleton"))?;
        let joint_index = skeleton
            .joints
            .iter()
            .position(|joint| joint.name.eq_ignore_ascii_case(joint_name))
            .ok_or_else(|| {
                format!(
                    "scene entity {stable_id} skeleton '{}' has no joint '{}'",
                    skeleton.name, joint_name
                )
            })?;
        let mut globals = Vec::<[f32; 16]>::with_capacity(skeleton.joints.len());
        for (index, joint) in skeleton.joints.iter().enumerate() {
            let local = trs_matrix(
                joint.bind_translation,
                joint.bind_rotation,
                joint.bind_scale,
            );
            let global = if let Some(parent) = joint.parent {
                let parent = parent as usize;
                if parent >= index {
                    return Err(format!(
                        "skeleton '{}' joint={} has non-topological parent={}",
                        skeleton.name, index, parent
                    ));
                }
                mul_mat4(globals[parent], local)
            } else {
                local
            };
            globals.push(global);
        }
        Ok(mul_mat4(*source_to_model, globals[joint_index]))
    }

    /// Head motion relative to the rig root, in camera pitch/heading/roll axes.
    /// Imported locomotion may replace the bind root's coordinate-system rotation.
    /// That conversion is not head bob and must not enter the camera springs.
    pub fn entity_joint_animation_delta_pose(
        &self,
        stable_id: u64,
        joint_name: &str,
    ) -> Result<([f32; 3], [f32; 3]), String> {
        let bind = self.bind_joint_model_matrix(stable_id, joint_name)?;
        let (skeleton, _) = self.entity_skeletons.get(&stable_id)
            .ok_or_else(|| "camera anchor has no installed skeleton".to_owned())?;
        let index = skeleton.joints.iter().position(|j| j.name.eq_ignore_ascii_case(joint_name))
            .ok_or_else(|| format!("camera anchor has no joint {joint_name}"))?;
        let mut root_index = index;
        while let Some(parent) = skeleton.joints[root_index].parent {
            let parent = parent as usize;
            if parent >= root_index { return Err("camera anchor skeleton is not topological".to_owned()); }
            root_index = parent;
        }
        let bind_root = self.bind_joint_model_matrix(stable_id, &skeleton.joints[root_index].name)?;
        let (animated, animated_root) = if self.skinned_entities.contains_key(&stable_id) {
            let pose = self.entity_pose_model_matrices(stable_id)?;
            (pose[index], pose[root_index])
        } else {
            (bind, bind_root)
        };
        Ok((
            std::array::from_fn(|i| animated[12 + i] - bind[12 + i]),
            joint_camera_animation_angles(bind, animated, bind_root, animated_root)?,
        ))
    }

    pub fn entity_joint_world_matrix(
        &self,
        stable_id: u64,
        joint_name: &str,
    ) -> Result<[f32; 16], String> {
        let model_joint = if self.skinned_entities.contains_key(&stable_id) {
            self.animated_joint_model_matrix(stable_id, joint_name)?
        } else {
            self.bind_joint_model_matrix(stable_id, joint_name)?
        };
        let entity = self
            .world
            .entity(SceneEntityId(stable_id))
            .ok_or_else(|| format!("scene entity {stable_id} disappeared"))?;
        let entity_matrix = geometry::instance_model_matrix(
            entity.transform.position,
            entity.transform.rotation_degrees,
            entity.transform.scale,
        );
        Ok(mul_mat4(entity_matrix, model_joint))
    }

    /// The foot markers are sampled by the SAME asynchronous worker that
    /// generated the currently displayed packed vertices. Using playback's
    /// newest simulation pose here used to play contact audio ahead of Vulkan.
    pub fn entity_animation_feet_positions(
        &self,
        stable_id: u64,
    ) -> Option<[[f32; 3]; 2]> {
        let state = self.skinned_entities.get(&stable_id)?;
        let playback = state.playback.as_ref()?;
        if state.pose_layers.is_physical()
            || state.presented_generation != Some(playback.generation)
        {
            return None;
        }
        let feet = state.presented_feet?;
        let entity = self.world.entity(SceneEntityId(stable_id))?;
        let transform = geometry::instance_model_matrix(
            entity.transform.position,
            entity.transform.rotation_degrees,
            entity.transform.scale,
        );
        Some(feet.map(|point| {
            let result = transform_point_mat4(transform, Vec3::new(point[0],point[1],point[2]));
            [result.x,result.y,result.z]
        }))
    }

    pub fn entity_joint_world_pose(
        &self,
        stable_id: u64,
        joint_name: &str,
    ) -> Result<([f32; 3], [f32; 3]), String> {
        let world_joint = self.entity_joint_world_matrix(stable_id, joint_name)?;
        Ok((
            [world_joint[12], world_joint[13], world_joint[14]],
            matrix_rotation_degrees(world_joint)?,
        ))
    }

    pub fn entity_joint_world_position(
        &self,
        stable_id: u64,
        joint_name: &str,
    ) -> Result<[f32; 3], String> {
        self.entity_joint_world_pose(stable_id, joint_name)
            .map(|(position, _)| position)
    }

    pub fn attach_entity_joint_to_joint(
        &mut self,
        child: u64,
        parent: u64,
        parent_joint: &str,
        child_joint: &str,
    ) -> Result<(), String> {
        self.attach_entity_joint_to_joint_offset(
            child,
            parent,
            parent_joint,
            child_joint,
            [0.0; 3],
            [0.0; 3],
        )
    }

    pub fn set_entity_joint_attachment(
        &mut self,
        child: u64,
        parent: u64,
        parent_joint: &str,
        child_joint: &str,
    ) -> Result<(), String> {
        self.set_entity_joint_attachment_offset(
            child,
            parent,
            parent_joint,
            child_joint,
            [0.0; 3],
            [0.0; 3],
        )
    }

    pub(super) fn attach_entity_joint_to_joint_offset(
        &mut self,
        child_stable_id: u64,
        parent_stable_id: u64,
        parent_joint: &str,
        child_joint: &str,
        position_offset: [f32; 3],
        rotation_offset_degrees: [f32; 3],
    ) -> Result<(), String> {
        // GTA's CPedEquippedWeapon::AttachObjects aligns the child's gun_gripr
        // matrix to the ped's PH_R_Hand by multiplying by the inverse child
        // offset-bone transform. Reproduce that relation directly.
        let parent_world = self.entity_joint_world_matrix(parent_stable_id, parent_joint)?;
        let offset = geometry::instance_model_matrix(
            Vec3::new(position_offset[0], position_offset[1], position_offset[2]),
            Vec3::new(
                rotation_offset_degrees[0],
                rotation_offset_degrees[1],
                rotation_offset_degrees[2],
            ),
            Vec3::new(1.0, 1.0, 1.0),
        );
        let parent_world = mul_mat4(parent_world, offset);
        let child_joint_model = if child_joint.is_empty() {
            trs_matrix([0.0; 3], [0.0, 0.0, 0.0, 1.0], [1.0; 3])
        } else {
            self.bind_joint_model_matrix(child_stable_id, child_joint)?
        };
        let child_joint_inverse = inverse_affine(child_joint_model).ok_or_else(|| {
            format!(
                "scene entity {child_stable_id} child joint '{}' has singular bind transform",
                child_joint
            )
        })?;
        let child_world = mul_mat4(parent_world, child_joint_inverse);
        let scale = matrix_scale(child_world)?;
        let rotation = matrix_rotation_degrees(child_world)?;
        self.set_entity_transform(
            child_stable_id,
            [child_world[12], child_world[13], child_world[14]],
            rotation,
            scale,
        )
    }

    /// Registers an engine-owned joint constraint. Unlike the old one-shot
    /// transform command, this survives asynchronous model streaming and is
    /// evaluated after animation sampling on every scene tick.
    pub fn set_entity_joint_attachment_offset(
        &mut self,
        child_stable_id: u64,
        parent_stable_id: u64,
        parent_joint: &str,
        child_joint: &str,
        position_offset: [f32; 3],
        rotation_offset_degrees: [f32; 3],
    ) -> Result<(), String> {
        if child_stable_id == parent_stable_id {
            return Err("joint attachment cannot parent an entity to itself".to_owned());
        }
        let parent_joint = parent_joint.trim();
        let child_joint = child_joint.trim();
        if parent_joint.is_empty()
            || position_offset
                .iter()
                .chain(rotation_offset_degrees.iter())
                .any(|v| !v.is_finite())
        {
            return Err("joint attachment requires non-empty parent and child joints".to_owned());
        }
        if self.world.entity(SceneEntityId(child_stable_id)).is_none() {
            return Err(format!("scene entity {child_stable_id} does not exist"));
        }
        if self.world.entity(SceneEntityId(parent_stable_id)).is_none() {
            return Err(format!("scene entity {parent_stable_id} does not exist"));
        }

        let attachment = SceneJointAttachment {
            parent_stable_id,
            parent_joint: parent_joint.to_owned(),
            child_joint: child_joint.to_owned(),
            position_offset,
            rotation_offset_degrees,
        };
        if self.joint_attachments.get(&child_stable_id) == Some(&attachment) {
            return Ok(());
        }
        self.joint_attachments.insert(child_stable_id, attachment);

        match self.attach_entity_joint_to_joint_offset(
            child_stable_id,
            parent_stable_id,
            parent_joint,
            child_joint,
            position_offset,
            rotation_offset_degrees,
        ) {
            Ok(()) => Ok(()),
            Err(error) if joint_attachment_pose_pending(&error) => Ok(()),
            Err(error) => {
                self.joint_attachments.remove(&child_stable_id);
                Err(error)
            }
        }
    }

    pub fn clear_entity_joint_attachment(&mut self, child_stable_id: u64) -> bool {
        self.joint_attachments.remove(&child_stable_id).is_some()
    }

    pub(in crate::first_scene) fn update_joint_attachments(&mut self) -> Result<(), String> {
        if self.joint_attachments.is_empty() {
            return Ok(());
        }

        let constraints = self
            .joint_attachments
            .iter()
            .map(|(child, attachment)| (*child, attachment.clone()))
            .collect::<Vec<_>>();
        let mut stale = Vec::new();

        for (child_stable_id, attachment) in constraints {
            if self.world.entity(SceneEntityId(child_stable_id)).is_none()
                || self
                    .world
                    .entity(SceneEntityId(attachment.parent_stable_id))
                    .is_none()
            {
                stale.push(child_stable_id);
                continue;
            }

            match self.attach_entity_joint_to_joint_offset(
                child_stable_id,
                attachment.parent_stable_id,
                &attachment.parent_joint,
                &attachment.child_joint,
                attachment.position_offset,
                attachment.rotation_offset_degrees,
            ) {
                Ok(()) => {}
                Err(error) if joint_attachment_pose_pending(&error) => {}
                Err(error) => return Err(error),
            }
        }

        for child_stable_id in stale {
            self.joint_attachments.remove(&child_stable_id);
        }
        Ok(())
    }
}

fn joint_attachment_pose_pending(error: &str) -> bool {
    error.contains("has no installed model skeleton")
        || error.contains("is not an installed skinned model")
}
