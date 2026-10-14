use super::*;
mod materials;
mod fragment_geometry;
mod fragments;
mod vertex_decode;
mod triangles;
mod bounds;
#[cfg(test)]
mod tests;
use fragment_geometry::*;
use vertex_decode::{decode_indices, read_vec2, read_vec3, read_vec4, read_skin_influences};
use triangles::expand_model_triangles;
pub(super) use triangles::append_local_asset_vertices;
use bounds::transformed_model_bounds;
pub(super) use bounds::transformed_local_bounds;
use newviso_materials::{BlendMode, MaterialParamValue, MaterialResource};
use newviso_model::{
    IndexFormat as ModelIndexFormat, ModelResource, VertexFormat as ModelVertexFormat,
    VertexSemantic,
};
use newviso_resource_runtime::AssetId;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct SceneResolvedMaterial {
    pub material: Arc<MaterialResource>,
    pub base_color: Option<Arc<TextureResource>>,
    pub normal: Option<Arc<TextureResource>>,
    pub specular: Option<Arc<TextureResource>>,
    pub emissive: Option<Arc<TextureResource>>,
    pub environment: Option<Arc<TextureResource>>,
    /// Authored samplers outside the generic PBR five-channel contract.
    ///
    /// Terrain/snow materials use semantic roles such as base_color_layer1,
    /// normal_layer2, height_layer3 and terrain_lookup. Keep these resolved
    /// resources alive through scene materialization instead of discarding them.
    pub auxiliary_textures: BTreeMap<String, Arc<TextureResource>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum AssetAlphaMode {
    Opaque,
    Cutout,
    Blend,
}

#[derive(Clone, Debug)]
pub(super) struct CpuAssetMaterial {
    pub(super) name: String,
    pub(super) textures: SceneResolvedMaterial,
    pub(super) normal_strength: f32,
    pub(super) specular_intensity: f32,
    pub(super) specular_falloff: f32,
    pub(super) specular_fresnel: f32,
    pub(super) emissive_multiplier: f32,
    pub(super) environment_reflection: f32,
    pub(super) alpha_mode: AssetAlphaMode,
    pub(super) alpha_cutoff: f32,
    pub(super) opacity: f32,
    pub(super) render_bucket: u32,
    pub(super) two_sided: bool,
    pub(super) shader_name: Option<String>,
    pub(super) use_vertex_color: bool,
    pub(super) metallic_roughness: bool,
    pub(super) metallic_factor: f32,
    pub(super) roughness_factor: f32,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct AssetTriangleVertex {
    pub(super) position: Vec3,
    pub(super) normal: Vec3,
    pub(super) tangent: [f32; 4],
    pub(super) color: [f32; 4],
    pub(super) uv: [f32; 2],
    pub(super) joints: [u16; 8],
    pub(super) weights: [f32; 8],
    pub(super) skin_influences: u8,
}

#[derive(Clone, Debug)]
pub(super) struct AssetDrawRange {
    pub(super) mesh_name: Arc<str>,
    pub(super) render_passes: newviso_model::MeshRenderPasses,
    pub(super) joint_lineage: Arc<[String]>,
    pub(super) first_vertex: u32,
    pub(super) vertex_count: u32,
    pub(super) material_slot: Option<u32>,
    /// Center of the triangles actually referenced by this draw range.
    pub(super) local_center: Vec3,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SceneModelDent {
    pub point: [f32; 3],
    pub displacement: [f32; 3],
    pub radius: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SceneModelPartPose {
    /// Exact semantic mesh names belonging to this articulated part.
    pub mesh_names: Vec<String>,
    /// Pivot in model-local space. Imported fragment rest positions use this.
    pub pivot: [f32; 3],
    /// Model-local translation relative to the imported rest pose.
    pub translation: [f32; 3],
    /// Model-local XYZ Euler delta in degrees.
    pub rotation_degrees: [f32; 3],
    /// Model-local scale delta. [1,1,1] preserves the imported rest shape.
    pub scale: [f32; 3],
    /// Scale applied after articulation rotation. Vehicle tyres use this for
    /// world-vertical flattening so the flat profile does not rotate with tread.
    pub post_rotation_scale: [f32; 3],
    /// Invisible parts are collapsed to their pivot, producing no raster area.
    pub visible: bool,
    pub dents: Vec<SceneModelDent>,
    /// Hit UV and accumulated glass damage, carried per vertex to the glass shader.
    pub glass_damage: Option<[f32; 3]>,
}

impl Default for SceneModelPartPose {
    fn default() -> Self {
        Self {
            mesh_names: Vec::new(),
            pivot: [0.0; 3],
            translation: [0.0; 3],
            rotation_degrees: [0.0; 3],
            scale: [1.0; 3],
            post_rotation_scale: [1.0; 3],
            visible: true,
            dents: Vec::new(),
            glass_damage: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ScenePreparedModelGeometry {
    model_id: u64,
    vertices: Arc<[AssetTriangleVertex]>,
    draw_ranges: Arc<[AssetDrawRange]>,
}

pub fn prepare_model_geometry(model: &ModelResource) -> Result<ScenePreparedModelGeometry, String> {
    let (vertices, ranges) = expand_model_triangles(model)?;
    if vertices.is_empty() {
        return Err(format!(
            "model '{}' contains no renderable triangles",
            model.name
        ));
    }
    Ok(ScenePreparedModelGeometry {
        model_id: model.id.0,
        vertices: Arc::from(vertices),
        draw_ranges: Arc::from(ranges),
    })
}

#[derive(Clone, Debug)]
pub(super) struct CpuAssetMesh {
    /// Source semantic model identity. Stable across static and skinned instances.
    pub(super) source_model_id: AssetId,
    /// Render geometry identity. Skinned entities receive a unique id so their
    /// deformed vertex range is never instanced with another pose.
    pub(super) model_id: AssetId,
    pub(super) local_bounds: SceneBounds,
    pub(super) local_draw_ranges: Arc<[AssetDrawRange]>,
    /// Render-phase classification is immutable for an installed model/material
    /// binding. Keep it out of the per-frame submission hot path.
    pub(super) opaque_draw_range_indices: Arc<[u32]>,
    pub(super) alpha_draw_range_indices: Arc<[u32]>,
    pub(super) materials: Arc<[CpuAssetMaterial]>,
    pub(super) first_vertex: u32,
    pub(super) vertex_count: u32,
    /// Compact offset into the frame-owned skinned vertex stream.
    /// Static meshes keep None and read from the persistent asset vertex buffer.
    pub(super) skinned_first_vertex: Option<u32>,
    /// Fragment models own an entity-local mutable vertex range so articulated
    /// wheels/panels can move without modifying another instance.
    pub(super) fragment_deformable: bool,
    pub(super) main_view_only: bool,
    pub(super) last_fragment_poses: std::collections::BTreeMap<Vec<String>, SceneModelPartPose>,
    /// Invalidated before fragment vertex writes; replaced with the installed mesh.
    pub(super) fragment_mesh_bounds: std::cell::RefCell<
        BTreeMap<Vec<String>, Option<([f32; 3], [f32; 3])>>,
    >,
}

impl Scene3dRuntime {
    pub fn install_entity_model(
        &mut self,
        stable_id: u64,
        model: &ModelResource,
        resolved_materials: &[SceneResolvedMaterial],
    ) -> Result<bool, String> {
        self.install_entity_model_prepared(stable_id, model, resolved_materials, None)
    }

    pub fn model_geometry_cached(&self, model_id: u64) -> bool {
        self.asset_model_cache.contains_key(&model_id)
            && self.asset_draw_range_cache.contains_key(&model_id)
    }

    pub fn install_entity_model_prepared(
        &mut self,
        stable_id: u64,
        model: &ModelResource,
        resolved_materials: &[SceneResolvedMaterial],
        prepared: Option<&ScenePreparedModelGeometry>,
    ) -> Result<bool, String> {
        if self
            .asset_meshes
            .get(&stable_id)
            .is_some_and(|mesh| mesh.source_model_id == model.id)
        {
            return Ok(false);
        }

        let id = SceneEntityId(stable_id);
        let transform = self
            .world
            .entity(id)
            .ok_or_else(|| format!("scene entity {} does not exist", stable_id))?
            .transform;

        let (local_vertices, local_draw_ranges) = if let (Some(vertices), Some(ranges)) = (
            self.asset_model_cache.get(&model.id.0),
            self.asset_draw_range_cache.get(&model.id.0),
        ) {
            (vertices.clone(), ranges.clone())
        } else {
            let owned;
            let prepared = match prepared {
                Some(prepared) => prepared,
                None => {
                    owned = prepare_model_geometry(model)?;
                    &owned
                }
            };
            if prepared.model_id != model.id.0 {
                return Err(format!(
                    "prepared model geometry identity mismatch expected={} got={}",
                    model.id.0, prepared.model_id
                ));
            }
            let vertices = prepared.vertices.clone();
            let ranges = prepared.draw_ranges.clone();
            self.asset_model_cache.insert(model.id.0, vertices.clone());
            self.asset_draw_range_cache
                .insert(model.id.0, ranges.clone());
            (vertices, ranges)
        };

        let skinned = model.skeleton.is_some()
            && local_vertices
                .iter()
                .any(|vertex| vertex.skin_influences != 0);
        let fragment_deformable = model.fragment.is_some();
        let entity_local_geometry = skinned || fragment_deformable;
        // Static geometry is shared by semantic model id. Skinned and articulated
        // fragment entities own unique mutable vertex ranges because their pose is
        // entity-local.
        let render_model_id = if entity_local_geometry {
            AssetId(model.id.0 ^ stable_id.rotate_left(23) ^ 0x534b_494e_4e45_4401_u64)
        } else {
            model.id
        };
        let mut skinned_first_vertex = None;
        let (first_vertex, vertex_count) = if skinned {
            let first_vertex = u32::try_from(self.asset_vertex_data.len() / FLOATS_PER_VERTEX)
                .map_err(|_| "skinned asset vertex offset exceeds u32".to_owned())?;
            let upload_from = self.asset_vertex_data.len();
            append_local_asset_vertices(&mut self.asset_vertex_data, &local_vertices);

            let compact_first =
                u32::try_from(self.skinned_vertex_data.len() / FLOATS_PER_VERTEX)
                    .map_err(|_| "compact skinned vertex offset exceeds u32".to_owned())?;
            let compact_start = self.skinned_vertex_data.len();
            append_local_asset_vertices(&mut self.skinned_vertex_data, &local_vertices);
            let compact_end = self.skinned_vertex_data.len();
            for ranges in &mut self.skinned_dirty_ranges {
                ranges.push((compact_start, compact_end));
            }
            skinned_first_vertex = Some(compact_first);

            let vertex_count = u32::try_from(local_vertices.len())
                .map_err(|_| "skinned asset vertex count exceeds u32".to_owned())?;
            self.asset_upload_from_float = Some(
                self.asset_upload_from_float
                    .map_or(upload_from, |existing| existing.min(upload_from)),
            );
            (first_vertex, vertex_count)
        } else if fragment_deformable {
            let first_vertex = u32::try_from(self.asset_vertex_data.len() / FLOATS_PER_VERTEX)
                .map_err(|_| "fragment asset vertex offset exceeds u32".to_owned())?;
            let upload_from = self.asset_vertex_data.len();
            append_local_asset_vertices(&mut self.asset_vertex_data, &local_vertices);
            let vertex_count = u32::try_from(local_vertices.len())
                .map_err(|_| "fragment asset vertex count exceeds u32".to_owned())?;
            self.asset_upload_from_float = Some(
                self.asset_upload_from_float
                    .map_or(upload_from, |existing| existing.min(upload_from)),
            );
            (first_vertex, vertex_count)
        } else if let Some(range) = self.asset_model_gpu_ranges.get(&model.id.0).copied() {
            range
        } else {
            let first_vertex = u32::try_from(self.asset_vertex_data.len() / FLOATS_PER_VERTEX)
                .map_err(|_| "static asset vertex offset exceeds u32".to_owned())?;
            let upload_from = self.asset_vertex_data.len();
            append_local_asset_vertices(&mut self.asset_vertex_data, &local_vertices);
            let vertex_count = u32::try_from(local_vertices.len())
                .map_err(|_| "static asset vertex count exceeds u32".to_owned())?;
            self.asset_model_gpu_ranges
                .insert(model.id.0, (first_vertex, vertex_count));
            self.asset_upload_from_float = Some(
                self.asset_upload_from_float
                    .map_or(upload_from, |existing| existing.min(upload_from)),
            );
            (first_vertex, vertex_count)
        };

        if resolved_materials.len() != model.material_slots.len() {
            return Err(format!(
                "model '{}' resolved material count {} does not match material slots {}",
                model.name,
                resolved_materials.len(),
                model.material_slots.len()
            ));
        }
        let materials: Arc<[CpuAssetMaterial]> = Arc::from(
            resolved_materials
                .iter()
                .map(CpuAssetMaterial::from_resolved)
                .collect::<Vec<_>>(),
        );
        let mut opaque_draw_range_indices = Vec::new();
        let mut alpha_draw_range_indices = Vec::new();
        for (range_index, range) in local_draw_ranges.iter().enumerate() {
            if !range.render_passes.color {
                continue;
            }
            let alpha_mode = range
                .material_slot
                .and_then(|slot| materials.get(slot as usize))
                .map(|material| material.alpha_mode)
                .unwrap_or(AssetAlphaMode::Opaque);
            let index = u32::try_from(range_index)
                .map_err(|_| "asset draw range index exceeds u32".to_owned())?;
            if alpha_mode == AssetAlphaMode::Blend {
                alpha_draw_range_indices.push(index);
            } else {
                opaque_draw_range_indices.push(index);
            }
        }
        let opaque_draw_range_indices: Arc<[u32]> = Arc::from(opaque_draw_range_indices);
        let alpha_draw_range_indices: Arc<[u32]> = Arc::from(alpha_draw_range_indices);

        let bounds = transformed_model_bounds(model, transform)?;
        self.asset_meshes.insert(
            stable_id,
            CpuAssetMesh {
                source_model_id: model.id,
                model_id: render_model_id,
                local_bounds: SceneBounds {
                    min: Vec3::new(
                        model.bounds.min[0],
                        model.bounds.min[1],
                        model.bounds.min[2],
                    ),
                    max: Vec3::new(
                        model.bounds.max[0],
                        model.bounds.max[1],
                        model.bounds.max[2],
                    ),
                },
                local_draw_ranges,
                opaque_draw_range_indices,
                alpha_draw_range_indices,
                materials,
                first_vertex,
                vertex_count,
                skinned_first_vertex,
                fragment_deformable,
                main_view_only: false,
                last_fragment_poses: std::collections::BTreeMap::new(),
                fragment_mesh_bounds: Default::default(),
            },
        );

        let installed_skeleton = model
            .skeleton
            .as_ref()
            .map(|skeleton| Arc::new(skeleton.clone()));
        if let Some(skeleton) = installed_skeleton.as_ref() {
            self.entity_skeletons
                .insert(stable_id, (skeleton.clone(), model.skin_source_to_model));
        } else {
            self.entity_skeletons.remove(&stable_id);
        }

        if skinned {
            self.skinned_entities.insert(
                stable_id,
                animation_skinning::SkinnedEntityAnimationState::new(
                    model.id,
                    model.skin_source_to_model,
                    installed_skeleton.expect("skinned model checked above"),
                )?,
            );
        } else {
            self.skinned_entities.remove(&stable_id);
        }

        if self
            .world
            .entity(id)
            .is_some_and(|entity| entity.mobility == SceneMobility::Static)
        {
            self.static_asset_instance_epoch = self.static_asset_instance_epoch.wrapping_add(1);
        }
        self.world
            .update_spatial_from(id, transform, bounds, SceneMutationSource::Streaming)?;
        Ok(true)
    }

    pub fn remove_entity_model(&mut self, stable_id: u64) -> bool {
        // Shared model geometry intentionally remains cached. Removing one
        // instance therefore creates no vertex-buffer hole and needs no rebuild.
        let persistent_static = self
            .world
            .entity(SceneEntityId(stable_id))
            .is_some_and(|entity| entity.mobility == SceneMobility::Static);
        let removed = self.asset_meshes.remove(&stable_id).is_some();
        self.skinned_entities.remove(&stable_id);
        self.entity_skeletons.remove(&stable_id);
        if removed && persistent_static {
            self.static_asset_instance_epoch = self.static_asset_instance_epoch.wrapping_add(1);
        }
        removed
    }

    pub fn entity_model_installed(&self, stable_id: u64) -> bool {
        self.asset_meshes.contains_key(&stable_id)
    }

    /// Installed geometry is eligible for scene visibility/draw submission.
    /// This does not mean it is currently inside the camera frustum.
    pub fn entity_model_renderable(&self, stable_id: u64) -> bool {
        self.asset_meshes
            .get(&stable_id)
            .is_some_and(|mesh| mesh.vertex_count > 0)
            && self
                .world
                .entity(SceneEntityId(stable_id))
                .is_some_and(|entity| entity.is_renderable())
    }

    pub(super) fn mark_entity_model_transform_dirty(&mut self, _stable_id: u64) {
        // SceneWorld owns transform mutation epochs. Static transforms invalidate
        // the persistent instance table; dynamic transforms update only the tail.
        // Local model geometry itself remains unchanged.
    }

    pub(super) fn rebuild_static_asset_vertex_data(&mut self) -> Result<(), String> {
        // Retained as a compatibility hook for renderer sync. Local geometry is
        // append-only and unique per ModelResource, so no compaction is needed.
        self.asset_geometry_full_rebuild = false;
        Ok(())
    }
}

