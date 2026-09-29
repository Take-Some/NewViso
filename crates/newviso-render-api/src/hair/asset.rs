use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{HairGroomRef, HairInstanceDescV1};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct HairGuidePointV1 {
    pub rest_position: [f32; 3],
    pub inverse_mass: f32,
}

impl HairGuidePointV1 {
    #[inline]
    pub fn normalized(mut self) -> Result<Self, String> {
        if !self.rest_position.iter().all(|value| value.is_finite()) {
            return Err("hair guide point contains non-finite rest position".to_owned());
        }
        self.inverse_mass = if self.inverse_mass.is_finite() {
            self.inverse_mass.clamp(0.0, 1_000.0)
        } else {
            1.0
        };
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct HairGuideStrandV1 {
    pub first_point: u32,
    pub point_count: u16,
    pub group: u16,
    pub root_uv: [f32; 2],
    /// Palette joint used to rigidly anchor this guide root to the animated skeleton.
    #[serde(default)]
    pub root_joint_index: u16,
}

impl HairGuideStrandV1 {
    #[inline]
    pub fn point_range(self) -> std::ops::Range<usize> {
        let start = self.first_point as usize;
        start..start.saturating_add(self.point_count as usize)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct HairCollisionCapsuleV1 {
    pub joint_index: u16,
    pub radius: f32,
    pub local_a: [f32; 3],
    pub local_b: [f32; 3],
}

impl HairCollisionCapsuleV1 {
    pub fn normalized(mut self) -> Result<Self, String> {
        if !self.local_a.iter().all(|value| value.is_finite())
            || !self.local_b.iter().all(|value| value.is_finite())
        {
            return Err("hair collision capsule contains non-finite endpoints".to_owned());
        }
        self.radius = if self.radius.is_finite() {
            self.radius.clamp(0.0001, 10_000.0)
        } else {
            0.05
        };
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HairGroomAssetV1 {
    pub groom: HairGroomRef,
    #[serde(default)]
    pub guide_points: Vec<HairGuidePointV1>,
    #[serde(default)]
    pub guide_strands: Vec<HairGuideStrandV1>,
    #[serde(default)]
    pub collision_capsules: Vec<HairCollisionCapsuleV1>,
    #[serde(default)]
    pub follow_strands_per_guide: u8,
}

impl HairGroomAssetV1 {
    pub fn normalized(mut self) -> Result<Self, String> {
        self.groom = self.groom.normalized()?;
        if self.guide_points.is_empty() {
            return Err(format!(
                "hair groom '{}' contains no guide points",
                self.groom.as_str()
            ));
        }
        if self.guide_points.len() > 1_048_576 {
            return Err(format!(
                "hair groom '{}' exceeds guide point safety limit",
                self.groom.as_str()
            ));
        }
        for point in &mut self.guide_points {
            *point = point.normalized()?;
        }
        if self.guide_strands.is_empty() {
            return Err(format!(
                "hair groom '{}' contains no guide strands",
                self.groom.as_str()
            ));
        }
        for strand in &self.guide_strands {
            if !(2..=256).contains(&strand.point_count) {
                return Err(format!(
                    "hair groom '{}' has strand point_count={} outside 2..=256",
                    self.groom.as_str(),
                    strand.point_count
                ));
            }
            if !strand.root_uv.iter().all(|value| value.is_finite()) {
                return Err(format!(
                    "hair groom '{}' has non-finite root UV",
                    self.groom.as_str()
                ));
            }
            let range = strand.point_range();
            if range.end > self.guide_points.len() {
                return Err(format!(
                    "hair groom '{}' strand range {}..{} exceeds guide point count {}",
                    self.groom.as_str(),
                    range.start,
                    range.end,
                    self.guide_points.len()
                ));
            }
        }
        let mut claimed = vec![false; self.guide_points.len()];
        for strand in &self.guide_strands {
            for point_index in strand.point_range() {
                if claimed[point_index] {
                    return Err(format!(
                        "hair groom '{}' guide strand ranges overlap at point {}",
                        self.groom.as_str(),
                        point_index
                    ));
                }
                claimed[point_index] = true;
            }
        }
        for capsule in &mut self.collision_capsules {
            *capsule = capsule.normalized()?;
        }
        self.follow_strands_per_guide = self.follow_strands_per_guide.min(16);
        Ok(self)
    }

    #[inline]
    pub fn guide_segment_count(&self) -> usize {
        self.guide_strands
            .iter()
            .map(|strand| usize::from(strand.point_count.saturating_sub(1)))
            .sum()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HairSkinPoseV1 {
    pub pose_id: u64,
    pub revision: u64,
    #[serde(default)]
    pub joint_deforms: Vec<[f32; 16]>,
}

impl HairSkinPoseV1 {
    pub fn normalized(mut self) -> Result<Self, String> {
        if self.pose_id == 0 {
            return Err("hair skin pose id must be non-zero".to_owned());
        }
        if self.joint_deforms.len() > u16::MAX as usize {
            return Err(format!(
                "hair skin pose {} exceeds {} joints",
                self.pose_id,
                u16::MAX
            ));
        }
        if self
            .joint_deforms
            .iter()
            .flatten()
            .any(|value| !value.is_finite())
        {
            return Err(format!(
                "hair skin pose {} contains non-finite matrix data",
                self.pose_id
            ));
        }
        self.revision = self.revision.max(1);
        Ok(self)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct HairSkinPoseRegistryV1 {
    poses: BTreeMap<u64, HairSkinPoseV1>,
    #[serde(skip)]
    layout_generation: u64,
}

impl HairSkinPoseRegistryV1 {
    pub fn upsert(&mut self, pose: HairSkinPoseV1) -> Result<(), String> {
        let pose = pose.normalized()?;
        let old_len = self
            .poses
            .get(&pose.pose_id)
            .map(|existing| existing.joint_deforms.len());
        let new_len = pose.joint_deforms.len();
        let is_new = old_len.is_none();
        self.poses.insert(pose.pose_id, pose);
        if is_new || old_len != Some(new_len) {
            self.layout_generation = self.layout_generation.wrapping_add(1);
        }
        Ok(())
    }

    #[inline]
    pub fn get(&self, pose_id: u64) -> Option<&HairSkinPoseV1> {
        self.poses.get(&pose_id)
    }

    #[inline]
    pub const fn layout_generation(&self) -> u64 {
        self.layout_generation
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.poses.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.poses.is_empty()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct HairGroomRegistryV1 {
    grooms: BTreeMap<String, HairGroomAssetV1>,
    #[serde(skip)]
    generation: u64,
}

impl HairGroomRegistryV1 {
    pub fn insert(&mut self, groom: HairGroomAssetV1) -> Result<(), String> {
        let groom = groom.normalized()?;
        self.grooms.insert(groom.groom.as_str().to_owned(), groom);
        self.generation = self.generation.wrapping_add(1);
        Ok(())
    }

    #[inline]
    pub fn get(&self, groom: &HairGroomRef) -> Option<&HairGroomAssetV1> {
        self.grooms.get(groom.as_str())
    }

    #[inline]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.grooms.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.grooms.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HairShaderSetV1 {
    pub simulation: String,
    pub strands_vertex: String,
    pub strands_fragment: String,
    /// Optional directional/CSM shadow caster shaders. Both must be present together.
    #[serde(default)]
    pub shadow_vertex: Option<String>,
    #[serde(default)]
    pub shadow_fragment: Option<String>,
}

impl HairShaderSetV1 {
    #[inline]
    pub fn new(
        simulation: impl Into<String>,
        strands_vertex: impl Into<String>,
        strands_fragment: impl Into<String>,
    ) -> Self {
        Self {
            simulation: simulation.into(),
            strands_vertex: strands_vertex.into(),
            strands_fragment: strands_fragment.into(),
            shadow_vertex: None,
            shadow_fragment: None,
        }
    }

    #[inline]
    pub fn with_shadows(
        mut self,
        shadow_vertex: impl Into<String>,
        shadow_fragment: impl Into<String>,
    ) -> Self {
        self.shadow_vertex = Some(shadow_vertex.into());
        self.shadow_fragment = Some(shadow_fragment.into());
        self
    }

    #[inline]
    pub fn has_shadows(&self) -> bool {
        self.shadow_vertex.is_some() && self.shadow_fragment.is_some()
    }

    pub fn normalized(mut self) -> Result<Self, String> {
        self.simulation = normalized_logical_shader_ref(&self.simulation)?;
        self.strands_vertex = normalized_logical_shader_ref(&self.strands_vertex)?;
        self.strands_fragment = normalized_logical_shader_ref(&self.strands_fragment)?;
        match (self.shadow_vertex.take(), self.shadow_fragment.take()) {
            (None, None) => {}
            (Some(vs), Some(fs)) => {
                self.shadow_vertex = Some(normalized_logical_shader_ref(&vs)?);
                self.shadow_fragment = Some(normalized_logical_shader_ref(&fs)?);
            }
            _ => {
                return Err(
                    "hair shadow shader set must provide both vertex and fragment shaders"
                        .to_owned(),
                );
            }
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HairSceneV1 {
    pub shaders: HairShaderSetV1,
    #[serde(default)]
    pub instances: Vec<HairInstanceDescV1>,
}

impl HairSceneV1 {
    #[inline]
    pub fn new(shaders: HairShaderSetV1) -> Self {
        Self {
            shaders,
            instances: Vec::new(),
        }
    }

    pub fn normalized(mut self) -> Result<Self, String> {
        self.shaders = self.shaders.normalized()?;
        let mut ids = std::collections::BTreeSet::new();
        for instance in &mut self.instances {
            *instance = instance.clone().normalized()?;
            if !ids.insert(instance.instance_id) {
                return Err(format!(
                    "duplicate hair instance_id={} in HairSceneV1",
                    instance.instance_id
                ));
            }
        }
        Ok(self)
    }

    #[inline]
    pub fn is_active(&self) -> bool {
        !self.instances.is_empty()
    }
}

fn normalized_logical_shader_ref(value: &str) -> Result<String, String> {
    let value = value.trim().replace('\\', "/");
    if value.is_empty() || value.len() > 512 {
        return Err("hair shader ref must contain 1..=512 bytes".to_owned());
    }
    if value.starts_with('/') || value.contains(":/") || value.contains("../") {
        return Err("hair shader ref must be a VFS-relative logical asset id".to_owned());
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiny_groom() -> HairGroomAssetV1 {
        HairGroomAssetV1 {
            groom: HairGroomRef::new("characters/test/hair.groom"),
            guide_points: vec![
                HairGuidePointV1 {
                    rest_position: [0.0, 0.0, 0.0],
                    inverse_mass: 0.0,
                },
                HairGuidePointV1 {
                    rest_position: [0.0, -0.1, 0.0],
                    inverse_mass: 1.0,
                },
                HairGuidePointV1 {
                    rest_position: [0.0, -0.2, 0.0],
                    inverse_mass: 1.0,
                },
            ],
            guide_strands: vec![HairGuideStrandV1 {
                first_point: 0,
                point_count: 3,
                group: 0,
                root_uv: [0.5, 0.5],
                root_joint_index: 0,
            }],
            collision_capsules: Vec::new(),
            follow_strands_per_guide: 4,
        }
    }

    #[test]
    fn groom_registry_validates_ranges_before_publish() {
        let mut registry = HairGroomRegistryV1::default();
        registry.insert(tiny_groom()).unwrap();
        assert_eq!(registry.len(), 1);
        assert_eq!(
            registry
                .get(&HairGroomRef::new("characters/test/hair.groom"))
                .unwrap()
                .guide_segment_count(),
            2
        );
    }

    #[test]
    fn skin_pose_registry_tracks_layout_not_revision() {
        let mut registry = HairSkinPoseRegistryV1::default();
        let identity = [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];
        registry
            .upsert(HairSkinPoseV1 {
                pose_id: 7,
                revision: 1,
                joint_deforms: vec![identity],
            })
            .unwrap();
        let layout = registry.layout_generation();
        registry
            .upsert(HairSkinPoseV1 {
                pose_id: 7,
                revision: 2,
                joint_deforms: vec![identity],
            })
            .unwrap();
        assert_eq!(registry.layout_generation(), layout);
        registry
            .upsert(HairSkinPoseV1 {
                pose_id: 7,
                revision: 3,
                joint_deforms: vec![identity, identity],
            })
            .unwrap();
        assert_ne!(registry.layout_generation(), layout);
    }

    #[test]
    fn shadow_shader_pair_is_atomic_and_vfs_relative() {
        let shaders = HairShaderSetV1::new(
            "shaders/hair/sim.comp",
            "shaders/hair/strands.vert",
            "shaders/hair/strands.frag",
        )
        .with_shadows(
            "shaders/hair/strand_shadow.vert",
            "shaders/hair/strand_shadow.frag",
        )
        .normalized()
        .unwrap();
        assert!(shaders.has_shadows());

        let partial = HairShaderSetV1 {
            shadow_vertex: Some("shaders/hair/strand_shadow.vert".to_owned()),
            shadow_fragment: None,
            ..HairShaderSetV1::new(
                "shaders/hair/sim.comp",
                "shaders/hair/strands.vert",
                "shaders/hair/strands.frag",
            )
        };
        assert!(partial.normalized().is_err());
        assert!(HairShaderSetV1::new(
            "shaders/hair/sim.comp",
            "shaders/hair/strands.vert",
            "shaders/hair/strands.frag",
        )
        .with_shadows("C:/sdk/hair_shadow.vert", "shaders/hair/shadow.frag")
        .normalized()
        .is_err());
    }

    #[test]
    fn shader_set_rejects_host_absolute_paths() {
        assert!(HairShaderSetV1::new(
            "C:/sdk/hair.comp",
            "shaders/hair/strands.vert",
            "shaders/hair/strands.frag"
        )
        .normalized()
        .is_err());
    }
}
