mod sweep;
pub use sweep::{sweep_sphere_aabb, SphereSweepMesh};

use newviso_resource_runtime::{AssetDomain, AssetId, AssetResource};
use std::{any::Any, sync::Arc};

pub const COLLISION_MESH_DOMAIN: AssetDomain = AssetDomain::new("engine.collision.mesh");

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CollisionBounds {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl CollisionBounds {
    pub fn is_finite(self) -> bool {
        self.min
            .iter()
            .chain(self.max.iter())
            .all(|value| value.is_finite())
    }
}

#[derive(Clone, Debug)]
pub struct CollisionMeshResource {
    pub id: AssetId,
    pub name: String,
    /// Local-space vertices. Placement transform is owned by Scene/World.
    pub vertices: Vec<[f32; 3]>,
    pub triangles: Vec<[u32; 3]>,
    /// RAGE physical material/type per triangle. Backends may remap this to
    /// engine materials without changing the imported topology.
    pub material_indices: Vec<u32>,
    pub bounds: CollisionBounds,
}

impl CollisionMeshResource {
    pub fn validate(&self) -> Result<(), String> {
        if self.vertices.is_empty() || self.triangles.is_empty() {
            return Err("collision mesh is empty".to_owned());
        }
        if !self.bounds.is_finite() {
            return Err("collision mesh bounds are not finite".to_owned());
        }
        if self
            .vertices
            .iter()
            .flatten()
            .any(|value| !value.is_finite())
        {
            return Err("collision mesh contains non-finite vertex".to_owned());
        }
        let vertex_count = self.vertices.len() as u64;
        if let Some((triangle_index, triangle)) =
            self.triangles.iter().enumerate().find(|(_, triangle)| {
                triangle
                    .iter()
                    .any(|index| u64::from(*index) >= vertex_count)
            })
        {
            return Err(format!(
                "collision triangle {} references vertex outside 0..{}: {:?}",
                triangle_index,
                self.vertices.len(),
                triangle
            ));
        }
        if !self.material_indices.is_empty() && self.material_indices.len() != self.triangles.len()
        {
            return Err(format!(
                "collision material count {} does not match triangle count {}",
                self.material_indices.len(),
                self.triangles.len()
            ));
        }
        Ok(())
    }
}

impl AssetResource for CollisionMeshResource {
    fn asset_id(&self) -> AssetId {
        self.id
    }

    fn domain(&self) -> AssetDomain {
        COLLISION_MESH_DOMAIN
    }

    fn into_any_arc(self: Arc<Self>) -> Arc<dyn Any + Send + Sync> {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_out_of_range_triangle() {
        let resource = CollisionMeshResource {
            id: AssetId(1),
            name: "bad".to_owned(),
            vertices: vec![[0.0; 3]],
            triangles: vec![[0, 1, 0]],
            material_indices: vec![0],
            bounds: CollisionBounds {
                min: [0.0; 3],
                max: [0.0; 3],
            },
        };
        assert!(resource.validate().is_err());
    }
}
