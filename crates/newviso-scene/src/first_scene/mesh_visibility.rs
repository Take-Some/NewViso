use super::*;
use std::collections::BTreeMap;

/// Per-instance main-view policy survives model streaming/reinstallation.
/// It never alters shared model geometry, skinning, physics or shadow draws.
#[derive(Debug, Default)]
pub(super) struct MainViewMeshVisibility {
    hidden_prefixes: BTreeMap<u64, Vec<String>>,
}

impl MainViewMeshVisibility {
    fn set(&mut self, id: u64, prefixes: Vec<String>) -> Result<(), String> {
        if prefixes.len() > 128 || prefixes.iter().any(|p| p.trim().is_empty() || p.len() > 256) {
            return Err("hidden_mesh_prefixes requires at most 128 non-empty names of at most 256 bytes".to_owned());
        }
        if prefixes.is_empty() {
            self.hidden_prefixes.remove(&id);
        } else {
            self.hidden_prefixes.insert(id, prefixes);
        }
        Ok(())
    }

    pub(super) fn remove(&mut self, id: u64) {
        self.hidden_prefixes.remove(&id);
    }

    pub(super) fn has_override(&self, id: u64) -> bool {
        self.hidden_prefixes.contains_key(&id)
    }

    pub(super) fn visible(&self, id: u64, mesh: &str) -> bool {
        !self.hidden_prefixes.get(&id).is_some_and(|prefixes| {
            prefixes.iter().any(|prefix| mesh.starts_with(prefix))
        })
    }
}

impl Scene3dRuntime {
    pub fn set_entity_main_view_hidden_meshes(
        &mut self,
        stable_id: u64,
        prefixes: Vec<String>,
    ) -> Result<(), String> {
        if self.world.entity(SceneEntityId(stable_id)).is_none() {
            return Err(format!("mesh visibility target {stable_id} does not exist"));
        }
        self.main_view_mesh_visibility.set(stable_id, prefixes)
    }
}

#[cfg(test)]
mod tests {
    use super::MainViewMeshVisibility;

    #[test]
    fn head_filter_is_per_instance_and_restores_on_clear() {
        let mut policy = MainViewMeshVisibility::default();
        policy.set(1, vec!["head_".into(), "hair_".into()]).unwrap();
        assert!(!policy.visible(1, "head_partition0"));
        assert!(!policy.visible(1, "hair_transparent"));
        assert!(policy.visible(1, "body"));
        assert!(policy.visible(1, "arms"));
        assert!(policy.visible(2, "head_partition0"));
        assert!(policy.has_override(1));
        assert!(!policy.has_override(2));
        policy.set(1, vec![]).unwrap();
        assert!(policy.visible(1, "head_partition0"));
        assert!(!policy.has_override(1));
    }

    #[test]
    fn policy_can_precede_residency_and_is_removed_with_entity() {
        let mut policy = MainViewMeshVisibility::default();
        policy.set(8, vec!["head_".into()]).unwrap();
        // Model installation/uninstallation does not own the policy.
        assert!(!policy.visible(8, "head_loaded_later"));
        policy.remove(8);
        assert!(policy.visible(8, "head_loaded_later"));
    }

    #[test]
    fn invalid_prefix_does_not_replace_existing_policy() {
        let mut policy = MainViewMeshVisibility::default();
        policy.set(1, vec!["head_".into()]).unwrap();
        assert!(policy.set(1, vec!["".into()]).is_err());
        assert!(!policy.visible(1, "head_partition0"));
        assert!(policy.visible(1, "body"));
    }
}
