use super::*;
use std::collections::BTreeMap;

#[derive(Debug, Default)]
struct MainViewPolicy {
    hidden_prefixes: Vec<String>,
    hidden_joint_roots: Vec<String>,
}

/// Instance-local draw filtering survives streaming and leaves shadow geometry intact.
#[derive(Debug, Default)]
pub(super) struct MainViewMeshVisibility {
    policies: BTreeMap<u64, MainViewPolicy>,
}

impl MainViewMeshVisibility {
    fn set(&mut self, id: u64, prefixes: Vec<String>, roots: Vec<String>) -> Result<(), String> {
        for (field, names) in [
            ("hidden_mesh_prefixes", &prefixes),
            ("hidden_joint_roots", &roots),
        ] {
            if names.len() > 128 || names.iter().any(|p| p.trim().is_empty() || p.len() > 256) {
                return Err(format!(
                    "{field} requires at most 128 non-empty names of at most 256 bytes"
                ));
            }
        }
        if prefixes.is_empty() && roots.is_empty() {
            self.policies.remove(&id);
        } else {
            self.policies.insert(
                id,
                MainViewPolicy {
                    hidden_prefixes: prefixes,
                    hidden_joint_roots: roots,
                },
            );
        }
        Ok(())
    }

    pub(super) fn remove(&mut self, id: u64) {
        self.policies.remove(&id);
    }
    pub(super) fn has_override(&self, id: u64) -> bool {
        self.policies.contains_key(&id)
    }

    /// Camera-local geometry must never become a reflected surface or a trace hit.
    pub(super) fn reflections_visible(&self, id: u64, main_view_only: bool) -> bool {
        !main_view_only && !self.has_override(id)
    }

    pub(super) fn visible(&self, id: u64, mesh: &str, lineage: &[String]) -> bool {
        !self.policies.get(&id).is_some_and(|policy| {
            policy
                .hidden_prefixes
                .iter()
                .any(|prefix| mesh.starts_with(prefix))
                || policy
                    .hidden_joint_roots
                    .iter()
                    .any(|root| lineage.iter().any(|joint| joint.eq_ignore_ascii_case(root)))
        })
    }
}

impl Scene3dRuntime {
    pub(super) fn asset_reflections_visible(&self, id: u64) -> bool {
        self.asset_meshes.get(&id).is_some_and(|mesh| {
            self.main_view_mesh_visibility.reflections_visible(id, mesh.main_view_only)
        })
    }

    pub(super) fn asset_instance_model_matrix(&self, id: u64, transform: SceneTransform) -> [f32; 16] {
        let mut matrix = geometry::instance_model_matrix(
            transform.position, transform.rotation_degrees, transform.scale,
        );
        // Instance transforms are affine. The sign of the otherwise constant
        // homogeneous lane carries the reflection policy; vertex shaders restore
        // +1 before any transform, including the full world shadow pass.
        if !self.asset_reflections_visible(id) {
            matrix[15] = -1.0;
        }
        matrix
    }

    pub(super) fn main_view_geometry_state(&self) -> Value {
        let instances = self
            .asset_meshes
            .iter()
            .filter(|(id, mesh)| {
                mesh.main_view_only || self.main_view_mesh_visibility.has_override(**id)
            })
            .map(|(id, mesh)| {
                let hidden = mesh
                    .local_draw_ranges
                    .iter()
                    .filter(|range| {
                        !self.main_view_mesh_visibility.visible(
                            *id,
                            &range.mesh_name,
                            &range.joint_lineage,
                        )
                    })
                    .map(|range| range.vertex_count as u64)
                    .sum::<u64>();
                let name = self
                    .runtime_entity_ids
                    .iter()
                    .find(|(_, stable)| **stable == *id)
                    .map(|(name, _)| name.as_str())
                    .unwrap_or("");
                json!({"id":name,"entity":id,"main_view_only":mesh.main_view_only,
                    "reflections_visible":self.asset_reflections_visible(*id),
                    "vertices":mesh.vertex_count,"hidden_vertices":hidden,
                    "visible_vertices":mesh.vertex_count as u64-hidden,
                    "in_frame":self.frame_plan.visible_entities.contains(&SceneEntityId(*id)),
                    "joint_roots":self.main_view_mesh_visibility.policies.get(id)
                        .map(|p| p.hidden_joint_roots.as_slice()).unwrap_or(&[]),
                })
            })
            .collect::<Vec<_>>();
        json!({"instances":instances})
    }

    pub fn set_entity_main_view_hidden_meshes(
        &mut self,
        stable_id: u64,
        prefixes: Vec<String>,
    ) -> Result<(), String> {
        self.set_entity_main_view_hidden_geometry(stable_id, prefixes, Vec::new())
    }

    pub fn set_entity_main_view_hidden_geometry(
        &mut self,
        stable_id: u64,
        prefixes: Vec<String>,
        roots: Vec<String>,
    ) -> Result<(), String> {
        if self.world.entity(SceneEntityId(stable_id)).is_none() {
            return Err(format!("mesh visibility target {stable_id} does not exist"));
        }
        let reflected_before = self.main_view_mesh_visibility.has_override(stable_id);
        self.main_view_mesh_visibility.set(stable_id, prefixes, roots)?;
        if reflected_before != self.main_view_mesh_visibility.has_override(stable_id)
            && self.world.entity(SceneEntityId(stable_id))
                .is_some_and(|entity| entity.mobility == SceneMobility::Static)
        {
            self.static_asset_instance_epoch = self.static_asset_instance_epoch.wrapping_add(1);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::MainViewMeshVisibility;

    #[test]
    fn mixed_mesh_head_filter_retains_arms_and_other_instances() {
        let mut policy = MainViewMeshVisibility::default();
        policy
            .set(1, vec!["hair_".into()], vec!["SKEL_Neck_1".into()])
            .unwrap();
        let head = vec![
            "FACIAL_Eye".into(),
            "SKEL_Head".into(),
            "SKEL_Neck_1".into(),
        ];
        let arm = vec!["SKEL_L_Hand".into(), "SKEL_L_Forearm".into()];
        assert!(!policy.visible(1, "shared_skin", &head));
        assert!(policy.visible(1, "shared_skin", &arm));
        assert!(policy.visible(2, "shared_skin", &head));
        assert!(!policy.visible(1, "hair_transparent", &[]));
        assert!(policy.has_override(1));
        policy.set(1, vec![], vec![]).unwrap();
        assert!(policy.visible(1, "shared_skin", &head));
        assert!(!policy.has_override(1));
    }

    #[test]
    fn first_person_reflection_filter_is_instance_local_and_reversible() {
        let mut policy = MainViewMeshVisibility::default();
        policy.set(11, vec![], vec!["SKEL_Neck_1".into()]).unwrap();
        let world_instances = [10, 11, 12, 13];
        let reflected = world_instances.into_iter()
            .filter(|id| policy.reflections_visible(*id, *id == 13)).collect::<Vec<_>>();
        assert_eq!(reflected, vec![10, 12]);
        assert!(policy.visible(11, "arms", &["SKEL_L_Hand".into()]));
        policy.set(11, vec![], vec![]).unwrap();
        assert!(policy.reflections_visible(11, false));
        assert!(!policy.reflections_visible(13, true));
    }

    #[test]
    fn policy_survives_residency_and_rejects_invalid_replacement() {
        let mut policy = MainViewMeshVisibility::default();
        policy.set(8, vec![], vec!["head".into()]).unwrap();
        assert!(policy.set(8, vec![], vec!["".into()]).is_err());
        assert!(!policy.visible(8, "loaded_later", &["head".into()]));
        policy.remove(8);
        assert!(policy.visible(8, "loaded_later", &["head".into()]));
    }
}
