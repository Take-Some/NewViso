use super::*;
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
}

fn material_float(material: &MaterialResource, name: &str, default: f32) -> f32 {
    material
        .params
        .iter()
        .find(|parameter| parameter.name.eq_ignore_ascii_case(name))
        .and_then(|parameter| match &parameter.value {
            MaterialParamValue::Float(value) if value.is_finite() => Some(*value),
            _ => None,
        })
        .unwrap_or(default)
}

fn material_u32(material: &MaterialResource, name: &str, default: u32) -> u32 {
    material
        .params
        .iter()
        .find(|parameter| parameter.name.eq_ignore_ascii_case(name))
        .and_then(|parameter| match parameter.value {
            MaterialParamValue::Int(value) if value >= 0 => Some(value as u32),
            _ => None,
        })
        .unwrap_or(default)
}

impl CpuAssetMaterial {
    fn from_resolved(resolved: &SceneResolvedMaterial) -> Self {
        let material = resolved.material.as_ref();
        let render_bucket = material_u32(material, "render_bucket", 0);
        let opacity = material_float(material, "opacity", 1.0).clamp(0.0, 1.0);
        let alpha_mode = match material.blend {
            BlendMode::Alpha | BlendMode::Additive => AssetAlphaMode::Blend,
            BlendMode::Masked => AssetAlphaMode::Cutout,
            BlendMode::Opaque if opacity < 0.999 || render_bucket == 1 || render_bucket >= 3 => {
                AssetAlphaMode::Blend
            }
            BlendMode::Opaque if render_bucket == 2 => AssetAlphaMode::Cutout,
            BlendMode::Opaque => AssetAlphaMode::Opaque,
        };

        let mut textures = resolved.clone();
        let emissive_multiplier = material_float(material, "emissive_multiplier", 0.0).max(0.0);
        if emissive_multiplier > 0.0 && textures.emissive.is_none() {
            textures.emissive = textures.base_color.clone();
        }
        let authored_environment = material_float(material, "environment_reflection", 0.0).max(0.0);
        let environment_reflection =
            if textures.environment.is_some() && authored_environment <= 0.0 {
                1.0
            } else {
                authored_environment
            };

        Self {
            name: material.name.clone(),
            normal_strength: material_float(material, "normal_strength", 1.0).max(0.0),
            specular_intensity: material_float(material, "specular_intensity", 0.0).max(0.0),
            specular_falloff: material_float(material, "specular_falloff", 32.0).clamp(1.0, 512.0),
            specular_fresnel: material_float(material, "specular_fresnel", 0.0).clamp(0.0, 1.0),
            emissive_multiplier,
            environment_reflection,
            alpha_mode,
            alpha_cutoff: material
                .alpha_cutoff
                .unwrap_or_else(|| material_float(material, "alpha_cutoff", 0.5))
                .clamp(0.0, 1.0),
            opacity,
            render_bucket,
            two_sided: material.two_sided,
            shader_name: Some(material.shader.clone()),
            textures,
        }
    }

    pub(super) fn uniform_data(&self) -> [f32; 12] {
        let mut flags = 0u32;
        if self.textures.normal.is_some() {
            flags |= 1;
        }
        if self.textures.specular.is_some() {
            flags |= 2;
        }
        if self.emissive_multiplier > 0.0 && self.textures.emissive.is_some() {
            flags |= 4;
        }
        if self.environment_reflection > 0.0 {
            flags |= 32;
        }
        if self.textures.environment.is_some() {
            flags |= 128;
        }
        if self.alpha_mode == AssetAlphaMode::Cutout {
            flags |= 8;
        }
        if self.alpha_mode == AssetAlphaMode::Blend {
            flags |= 16;
        }
        [
            self.normal_strength,
            self.specular_intensity,
            self.specular_falloff,
            self.specular_fresnel,
            self.emissive_multiplier,
            self.alpha_cutoff,
            flags as f32,
            self.opacity,
            self.environment_reflection,
            0.0,
            0.0,
            0.0,
        ]
    }
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

fn apply_fragment_dents(
    mut p: Vec3,
    mut n: Vec3,
    mut t: Vec3,
    dents: &[SceneModelDent],
) -> (Vec3, Vec3, Vec3) {
    for dent in dents {
        let centre = Vec3::new(dent.point[0], dent.point[1], dent.point[2]);
        let delta = Vec3::new(
            dent.displacement[0],
            dent.displacement[1],
            dent.displacement[2],
        );
        let v = p.sub(centre);
        let radius_sq = dent.radius * dent.radius;
        let q = (v.x * v.x + v.y * v.y + v.z * v.z) / radius_sq.max(0.0001);
        if q >= 1.0 {
            continue;
        }
        let w = (1.0 - q).powi(2);
        let g = Vec3::new(v.x, v.y, v.z).mul(-4.0 * (1.0 - q) / radius_sq);
        let dot = |a: Vec3, b: Vec3| a.x * b.x + a.y * b.y + a.z * b.z;
        let denominator = (1.0 + dot(g, delta)).max(0.2);
        n = n.sub(g.mul(dot(delta, n) / denominator)).normalized();
        t = t.add(delta.mul(dot(g, t))).normalized();
        p = p.add(delta.mul(w));
    }
    (p, n, t)
}

/// Closest point including edges, with barycentrics for the original glass UVs.
fn closest_fragment_triangle(p: [f32; 3], t: [[f32; 3]; 3]) -> Option<([f32; 3], [f32; 3])> {
    let sub = |a: [f32; 3], b: [f32; 3]| std::array::from_fn(|i| a[i] - b[i]);
    let dot = |a: [f32; 3], b: [f32; 3]| (0..3).map(|i| a[i] * b[i]).sum::<f32>();
    let ab = sub(t[1], t[0]);
    let ac = sub(t[2], t[0]);
    let ap = sub(p, t[0]);
    let d00 = dot(ab, ab);
    let d01 = dot(ab, ac);
    let d11 = dot(ac, ac);
    let determinant = d00 * d11 - d01 * d01;
    if determinant <= 1.0e-12 {
        return None;
    }
    let u = (d11 * dot(ap, ab) - d01 * dot(ap, ac)) / determinant;
    let v = (d00 * dot(ap, ac) - d01 * dot(ap, ab)) / determinant;
    if u >= 0.0 && v >= 0.0 && u + v <= 1.0 {
        let weights = [1.0 - u - v, u, v];
        return Some((
            std::array::from_fn(|a| (0..3).map(|i| weights[i] * t[i][a]).sum()),
            weights,
        ));
    }
    let mut best = None;
    for (i, j) in [(0, 1), (1, 2), (2, 0)] {
        let edge = sub(t[j], t[i]);
        let f = (dot(sub(p, t[i]), edge) / dot(edge, edge).max(1.0e-12)).clamp(0.0, 1.0);
        let hit = std::array::from_fn(|a| t[i][a] + edge[a] * f);
        let distance = dot(sub(p, hit), sub(p, hit));
        let mut weights = [0.0; 3];
        weights[i] = 1.0 - f;
        weights[j] = f;
        if best.as_ref().is_none_or(|(d, _, _)| distance < *d) {
            best = Some((distance, hit, weights));
        }
    }
    best.map(|(_, hit, weights)| (hit, weights))
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

    pub fn entity_fragment_mesh_bounds(
        &self,
        entity: u64,
        names: &[String],
    ) -> Option<([f32; 3], [f32; 3])> {
        let mesh = self.asset_meshes.get(&entity)?;
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        let mut found = false;
        for range in mesh.local_draw_ranges.iter().filter(|range| {
            names
                .iter()
                .any(|name| name.as_str() == range.mesh_name.as_ref())
        }) {
            let start =
                (mesh.first_vertex as usize + range.first_vertex as usize) * FLOATS_PER_VERTEX;
            let end = start + range.vertex_count as usize * FLOATS_PER_VERTEX;
            for vertex in self
                .asset_vertex_data
                .get(start..end)?
                .chunks_exact(FLOATS_PER_VERTEX)
            {
                for axis in 0..3 {
                    min[axis] = min[axis].min(vertex[axis]);
                    max[axis] = max[axis].max(vertex[axis]);
                }
                found = true;
            }
        }
        found.then_some((min, max))
    }

    /// Nearest non-degenerate live fragment triangle: squared distance, point and UV.
    pub fn entity_fragment_surface_hit(
        &self,
        entity: u64,
        names: &[String],
        point: [f32; 3],
    ) -> Option<(f32, [f32; 3], [f32; 2])> {
        let mesh = self.asset_meshes.get(&entity)?;
        let mut nearest = None;
        for range in mesh
            .local_draw_ranges
            .iter()
            .filter(|r| names.iter().any(|n| n.as_str() == r.mesh_name.as_ref()))
        {
            let start =
                (mesh.first_vertex as usize + range.first_vertex as usize) * FLOATS_PER_VERTEX;
            let end = start + range.vertex_count as usize * FLOATS_PER_VERTEX;
            for triangle in self
                .asset_vertex_data
                .get(start..end)?
                .chunks_exact(3 * FLOATS_PER_VERTEX)
            {
                let vertices: [[f32; 3]; 3] = std::array::from_fn(|i| {
                    std::array::from_fn(|a| triangle[i * FLOATS_PER_VERTEX + a])
                });
                let Some((hit, weights)) = closest_fragment_triangle(point, vertices) else {
                    continue;
                };
                let distance = (0..3).map(|a| (point[a] - hit[a]).powi(2)).sum::<f32>();
                if nearest.as_ref().is_none_or(|(best, _, _)| distance < *best) {
                    let uv = std::array::from_fn(|a| {
                        (0..3)
                            .map(|i| weights[i] * triangle[i * FLOATS_PER_VERTEX + 11 + a])
                            .sum()
                    });
                    nearest = Some((distance, hit, uv));
                }
            }
        }
        nearest
    }

    /// Copy only the selected live fragment geometry into a separate dynamic
    /// entity. Materials and textures are shared; vertices and bounds are local
    /// to the detached body, independent of the source vehicle's visibility.
    pub fn install_detached_fragment(
        &mut self,
        source: u64,
        target: u64,
        mesh_names: &[String],
    ) -> Result<([f32; 3], [f32; 3], Vec<[f32; 3]>), String> {
        let source_mesh = self
            .asset_meshes
            .get(&source)
            .cloned()
            .ok_or_else(|| format!("fragment source {source} has no model"))?;
        if self.world.entity(SceneEntityId(target)).is_none() {
            return Err(format!("fragment target {target} does not exist"));
        }
        let mut data = Vec::new();
        let mut ranges = Vec::new();
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        for range in source_mesh.local_draw_ranges.iter().filter(|range| {
            mesh_names
                .iter()
                .any(|name| name.as_str() == range.mesh_name.as_ref())
        }) {
            let first = data.len() / FLOATS_PER_VERTEX;
            let start = (source_mesh.first_vertex as usize + range.first_vertex as usize)
                * FLOATS_PER_VERTEX;
            let end = start + range.vertex_count as usize * FLOATS_PER_VERTEX;
            let vertices = self
                .asset_vertex_data
                .get(start..end)
                .ok_or_else(|| "fragment source vertex range is invalid".to_owned())?;
            for vertex in vertices.chunks_exact(FLOATS_PER_VERTEX) {
                for axis in 0..3 {
                    min[axis] = min[axis].min(vertex[axis]);
                    max[axis] = max[axis].max(vertex[axis]);
                }
            }
            data.extend_from_slice(vertices);
            ranges.push(AssetDrawRange {
                first_vertex: u32::try_from(first)
                    .map_err(|_| "fragment vertex offset overflow".to_owned())?,
                local_center: {
                    let mut sum = Vec3::ZERO;
                    for vertex in vertices.chunks_exact(FLOATS_PER_VERTEX) {
                        sum = sum.add(Vec3::new(vertex[0], vertex[1], vertex[2]));
                    }
                    sum.mul(1.0 / (vertices.len() / FLOATS_PER_VERTEX).max(1) as f32)
                },
                ..range.clone()
            });
        }
        if data.is_empty() {
            return Err("fragment has no separately bound render geometry".to_owned());
        }
        let center: [f32; 3] = std::array::from_fn(|axis| (min[axis] + max[axis]) * 0.5);
        let extent: [f32; 3] =
            std::array::from_fn(|axis| ((max[axis] - min[axis]) * 0.5).max(0.025));
        for range in &mut ranges {
            range.local_center = range
                .local_center
                .sub(Vec3::new(center[0], center[1], center[2]));
        }
        let mut hull = Vec::new();
        let mut hull_keys = std::collections::BTreeSet::new();
        for vertex in data.chunks_exact_mut(FLOATS_PER_VERTEX) {
            for axis in 0..3 {
                vertex[axis] -= center[axis];
            }
            let point = [vertex[0], vertex[1], vertex[2]];
            if hull_keys.insert(point.map(f32::to_bits)) {
                hull.push(point);
            }
        }
        let first_vertex = u32::try_from(self.asset_vertex_data.len() / FLOATS_PER_VERTEX)
            .map_err(|_| "fragment GPU vertex offset overflow".to_owned())?;
        let vertex_count = u32::try_from(data.len() / FLOATS_PER_VERTEX)
            .map_err(|_| "fragment vertex count overflow".to_owned())?;
        let upload_from = self.asset_vertex_data.len();
        self.asset_vertex_data.extend_from_slice(&data);
        self.asset_upload_from_float = Some(
            self.asset_upload_from_float
                .map_or(upload_from, |old| old.min(upload_from)),
        );
        let mut opaque = Vec::new();
        let mut alpha = Vec::new();
        for (index, range) in ranges.iter().enumerate() {
            let mode = range
                .material_slot
                .and_then(|slot| source_mesh.materials.get(slot as usize))
                .map(|material| material.alpha_mode)
                .unwrap_or(AssetAlphaMode::Opaque);
            if mode == AssetAlphaMode::Blend {
                alpha.push(index as u32);
            } else {
                opaque.push(index as u32);
            }
        }
        self.asset_meshes.insert(
            target,
            CpuAssetMesh {
                source_model_id: source_mesh.source_model_id,
                model_id: AssetId(
                    source_mesh.source_model_id.0 ^ target.rotate_left(23) ^ 0x4652_4147_4d45_4e54,
                ),
                local_bounds: SceneBounds {
                    min: Vec3::new(-extent[0], -extent[1], -extent[2]),
                    max: Vec3::new(extent[0], extent[1], extent[2]),
                },
                local_draw_ranges: Arc::from(ranges),
                opaque_draw_range_indices: Arc::from(opaque),
                alpha_draw_range_indices: Arc::from(alpha),
                materials: source_mesh.materials,
                first_vertex,
                vertex_count,
                skinned_first_vertex: None,
                fragment_deformable: false,
                main_view_only: false,
                last_fragment_poses: std::collections::BTreeMap::new(),
            },
        );
        Ok((center, extent, hull))
    }

    pub fn set_entity_model_part_poses(
        &mut self,
        stable_id: u64,
        poses: &[SceneModelPartPose],
    ) -> Result<usize, String> {
        if poses.is_empty() {
            return Ok(0);
        }
        let mesh = self
            .asset_meshes
            .get(&stable_id)
            .ok_or_else(|| format!("scene entity {stable_id} has no installed model"))?;
        if !mesh.fragment_deformable {
            return Err(format!(
                "scene entity {stable_id} model is not fragment-deformable"
            ));
        }
        if mesh.skinned_first_vertex.is_some() {
            return Err(format!(
                "scene entity {stable_id} cannot use fragment CPU poses while skinning is active"
            ));
        }
        let previous_poses = mesh.last_fragment_poses.clone();
        let source_model_id = mesh.source_model_id.0;
        let first_vertex = mesh.first_vertex as usize;
        let local_draw_ranges = mesh.local_draw_ranges.clone();
        let bind_vertices = self
            .asset_model_cache
            .get(&source_model_id)
            .cloned()
            .ok_or_else(|| {
                format!(
                    "scene entity {stable_id} fragment bind geometry model={source_model_id} is missing"
                )
            })?;

        let mut modified_vertices = 0usize;
        let mut first_modified_float = None::<usize>;
        for pose in poses {
            if previous_poses
                .get(&pose.mesh_names)
                .is_some_and(|previous| previous == pose)
            {
                continue;
            }
            if pose
                .glass_damage
                .is_some_and(|v| v.iter().any(|x| !x.is_finite()) || !(0.0..=1.0).contains(&v[2]))
            {
                return Err("invalid glass damage appearance".to_owned());
            }
            if pose
                .pivot
                .iter()
                .chain(pose.translation.iter())
                .chain(pose.rotation_degrees.iter())
                .chain(pose.scale.iter())
                .chain(pose.post_rotation_scale.iter())
                .any(|value| !value.is_finite())
                || pose.post_rotation_scale.iter().any(|value| *value <= 0.0)
            {
                return Err(format!(
                    "scene entity {stable_id} fragment pose contains non-finite values"
                ));
            }
            let pivot = Vec3::new(pose.pivot[0], pose.pivot[1], pose.pivot[2]);
            let translation = Vec3::new(
                pose.translation[0],
                pose.translation[1],
                pose.translation[2],
            );
            let rotation = Vec3::new(
                pose.rotation_degrees[0],
                pose.rotation_degrees[1],
                pose.rotation_degrees[2],
            );
            let authored_scale = if pose.visible {
                Vec3::new(pose.scale[0], pose.scale[1], pose.scale[2])
            } else {
                Vec3::ZERO
            };
            let post_scale = Vec3::new(
                pose.post_rotation_scale[0],
                pose.post_rotation_scale[1],
                pose.post_rotation_scale[2],
            );
            let pose_center = pivot.add(translation);

            for range in local_draw_ranges.iter().filter(|range| {
                pose.mesh_names
                    .iter()
                    .any(|name| name.as_str() == range.mesh_name.as_ref())
            }) {
                let local_first = range.first_vertex as usize;
                let local_end = local_first
                    .checked_add(range.vertex_count as usize)
                    .ok_or_else(|| "fragment vertex range overflow".to_owned())?;
                if local_end > bind_vertices.len() {
                    return Err(format!(
                        "scene entity {stable_id} fragment range {}..{} exceeds bind vertices={}",
                        local_first,
                        local_end,
                        bind_vertices.len()
                    ));
                }
                for local_index in local_first..local_end {
                    let bind = bind_vertices[local_index];
                    let centered = bind.position.sub(pivot);
                    let mut position =
                        transform_point(centered, authored_scale, rotation, pose_center);
                    let mut normal = if pose.visible {
                        transform_point(bind.normal, Vec3::new(1.0, 1.0, 1.0), rotation, Vec3::ZERO)
                            .normalized()
                    } else {
                        bind.normal
                    };
                    let tangent_vec = Vec3::new(bind.tangent[0], bind.tangent[1], bind.tangent[2]);
                    let mut tangent = if pose.visible {
                        transform_point(tangent_vec, Vec3::new(1.0, 1.0, 1.0), rotation, Vec3::ZERO)
                            .normalized()
                    } else {
                        tangent_vec
                    };
                    if pose.visible && pose.post_rotation_scale != [1.0; 3] {
                        let relative = position.sub(pose_center);
                        position = pose_center.add(Vec3::new(
                            relative.x * post_scale.x,
                            relative.y * post_scale.y,
                            relative.z * post_scale.z,
                        ));
                        normal = Vec3::new(
                            normal.x / post_scale.x,
                            normal.y / post_scale.y,
                            normal.z / post_scale.z,
                        )
                        .normalized();
                        tangent = Vec3::new(
                            tangent.x * post_scale.x,
                            tangent.y * post_scale.y,
                            tangent.z * post_scale.z,
                        )
                        .normalized();
                    }

                    if pose.visible && !pose.dents.is_empty() {
                        (position, normal, tangent) =
                            apply_fragment_dents(position, normal, tangent, &pose.dents);
                    }
                    let global_vertex = first_vertex
                        .checked_add(local_index)
                        .ok_or_else(|| "fragment global vertex overflow".to_owned())?;
                    let base = global_vertex
                        .checked_mul(FLOATS_PER_VERTEX)
                        .ok_or_else(|| "fragment vertex float offset overflow".to_owned())?;
                    let end = base + FLOATS_PER_VERTEX;
                    if end > self.asset_vertex_data.len() {
                        return Err(format!(
                            "scene entity {stable_id} fragment write {}..{} exceeds vertex floats={}",
                            base,
                            end,
                            self.asset_vertex_data.len()
                        ));
                    }
                    let dst = &mut self.asset_vertex_data[base..end];
                    dst[0] = position.x;
                    dst[1] = position.y;
                    dst[2] = position.z;
                    dst[4] = normal.x;
                    dst[5] = normal.y;
                    dst[6] = normal.z;
                    dst[7..11].copy_from_slice(&bind.color);
                    if let Some([u, v, damage]) = pose.glass_damage.filter(|v| v[2] > 0.0) {
                        dst[7..11].copy_from_slice(&[u, v, damage, -1.0]);
                    }
                    dst[13] = tangent.x;
                    dst[14] = tangent.y;
                    dst[15] = tangent.z;
                    first_modified_float =
                        Some(first_modified_float.map_or(base, |value| value.min(base)));
                    modified_vertices = modified_vertices.saturating_add(1);
                }
            }
        }
        if let Some(mesh) = self.asset_meshes.get_mut(&stable_id) {
            for pose in poses {
                mesh.last_fragment_poses
                    .insert(pose.mesh_names.clone(), pose.clone());
            }
        }
        if let Some(offset) = first_modified_float {
            self.asset_upload_from_float = Some(
                self.asset_upload_from_float
                    .map_or(offset, |existing| existing.min(offset)),
            );
        }
        Ok(modified_vertices)
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

pub(super) fn append_local_asset_vertices(out: &mut Vec<f32>, vertices: &[AssetTriangleVertex]) {
    out.reserve(vertices.len().saturating_mul(FLOATS_PER_VERTEX));
    for vertex in vertices {
        geometry::append_local_asset_vertex(
            out,
            vertex.position,
            vertex.normal,
            vertex.tangent,
            vertex.color,
            vertex.uv,
        );
    }
}

fn expand_model_triangles(
    model: &ModelResource,
) -> Result<(Vec<AssetTriangleVertex>, Vec<AssetDrawRange>), String> {
    let mut out = Vec::new();
    let mut draw_ranges = Vec::new();

    for mesh in &model.meshes {
        let position = mesh
            .vertex_streams
            .iter()
            .find(|stream| stream.semantic == VertexSemantic::Position)
            .ok_or_else(|| format!("mesh '{}' has no position stream", mesh.name))?;
        if position.format != ModelVertexFormat::Float32x3 {
            return Err(format!(
                "mesh '{}' position stream must be Float32x3",
                mesh.name
            ));
        }

        let normal = mesh
            .vertex_streams
            .iter()
            .find(|stream| stream.semantic == VertexSemantic::Normal);
        let tangent = mesh
            .vertex_streams
            .iter()
            .find(|stream| stream.semantic == VertexSemantic::Tangent);
        let color = mesh
            .vertex_streams
            .iter()
            .find(|stream| stream.semantic == VertexSemantic::Color(0));
        let texcoord = mesh
            .vertex_streams
            .iter()
            .find(|stream| stream.semantic == VertexSemantic::TexCoord(0));
        let joint_indices = mesh
            .vertex_streams
            .iter()
            .find(|stream| stream.semantic == VertexSemantic::JointIndices);
        let joint_weights = mesh
            .vertex_streams
            .iter()
            .find(|stream| stream.semantic == VertexSemantic::JointWeights);
        let joint_indices_extra = mesh
            .vertex_streams
            .iter()
            .find(|stream| stream.semantic == VertexSemantic::JointIndicesExtra);
        let joint_weights_extra = mesh
            .vertex_streams
            .iter()
            .find(|stream| stream.semantic == VertexSemantic::JointWeightsExtra);
        if joint_indices.is_some() != joint_weights.is_some()
            || joint_indices_extra.is_some() != joint_weights_extra.is_some()
        {
            return Err(format!(
                "mesh '{}' has incomplete joint index/weight stream pairs",
                mesh.name
            ));
        }

        let indices = decode_indices(&mesh.index_buffer)?;
        let primitives = if mesh.primitives.is_empty() {
            vec![(0usize, indices.len(), 0i32, None)]
        } else {
            mesh.primitives
                .iter()
                .map(|primitive| {
                    (
                        primitive.first_index as usize,
                        primitive.index_count as usize,
                        primitive.base_vertex,
                        primitive.material_slot,
                    )
                })
                .collect::<Vec<_>>()
        };

        for (first, count, base_vertex, material_slot) in primitives {
            let range_first_vertex = u32::try_from(out.len())
                .map_err(|_| "expanded model vertex offset exceeds u32".to_owned())?;
            let end = first
                .checked_add(count)
                .ok_or_else(|| format!("mesh '{}' index range overflows", mesh.name))?;
            if end > indices.len() {
                return Err(format!(
                    "mesh '{}' primitive index range {}..{} exceeds {} indices",
                    mesh.name,
                    first,
                    end,
                    indices.len()
                ));
            }

            let mut partitions = BTreeMap::<Option<u16>, Vec<AssetTriangleVertex>>::new();
            for tri in indices[first..end].chunks_exact(3) {
                let mut triangle = [AssetTriangleVertex {
                    position: Vec3::ZERO,
                    normal: Vec3::ZERO,
                    tangent: [1.0, 0.0, 0.0, 1.0],
                    color: [1.0, 1.0, 1.0, 1.0],
                    uv: [0.0, 0.0],
                    joints: [0; 8],
                    weights: [0.0; 8],
                    skin_influences: 0,
                }; 3];

                for (corner, raw_index) in tri.iter().copied().enumerate() {
                    let vertex_index = i64::from(raw_index) + i64::from(base_vertex);
                    if vertex_index < 0 {
                        return Err(format!(
                            "mesh '{}' primitive references negative vertex {}",
                            mesh.name, vertex_index
                        ));
                    }
                    let vertex_index = vertex_index as usize;
                    triangle[corner].position = read_vec3(position, vertex_index)?;
                    if let Some(stream) = normal {
                        triangle[corner].normal = read_vec3(stream, vertex_index)?;
                    }
                    if let Some(stream) = tangent {
                        triangle[corner].tangent = read_vec4(stream, vertex_index)?;
                    }
                    if let Some(stream) = color {
                        triangle[corner].color = read_vec4(stream, vertex_index)?;
                    }
                    if let Some(stream) = texcoord {
                        triangle[corner].uv = read_vec2(stream, vertex_index)?;
                    }
                    if let (Some(indices_stream), Some(weights_stream)) =
                        (joint_indices, joint_weights)
                    {
                        read_skin_influences(
                            &mut triangle[corner],
                            indices_stream,
                            weights_stream,
                            joint_indices_extra,
                            joint_weights_extra,
                            vertex_index,
                        )?;
                    }
                }

                if normal.is_none() {
                    let edge_a = triangle[1].position.sub(triangle[0].position);
                    let edge_b = triangle[2].position.sub(triangle[0].position);
                    let face_normal = edge_a.cross(edge_b).normalized();
                    for vertex in &mut triangle {
                        vertex.normal = face_normal;
                    }
                }
                if tangent.is_none() {
                    generate_triangle_tangent(&mut triangle);
                }
                let joint = dominant_triangle_joint(&triangle, model.skeleton.as_ref());
                partitions
                    .entry(joint)
                    .or_default()
                    .extend_from_slice(&triangle);
            }
            let _ = range_first_vertex;
            for (joint, vertices) in partitions {
                let first_vertex = u32::try_from(out.len())
                    .map_err(|_| "expanded model vertex offset exceeds u32".to_owned())?;
                let vertex_count = u32::try_from(vertices.len())
                    .map_err(|_| "expanded model partition exceeds u32".to_owned())?;
                out.extend_from_slice(&vertices);
                if vertex_count > 0 {
                    draw_ranges.push(AssetDrawRange {
                        mesh_name: Arc::from(mesh.name.as_str()),
                        joint_lineage: Arc::from(joint_lineage(model.skeleton.as_ref(), joint)),
                        first_vertex,
                        vertex_count,
                        material_slot,
                        local_center: vertices
                            .iter()
                            .fold(Vec3::ZERO, |sum, v| sum.add(v.position))
                            .mul(1.0 / vertices.len() as f32),
                    });
                }
            }
        }
    }
    Ok((out, draw_ranges))
}

// Skin components can share one mesh/material (head and hands are common).
// Partition by dominant skeletal influence once, at geometry preparation.
fn dominant_triangle_joint(
    triangle: &[AssetTriangleVertex; 3],
    skeleton: Option<&newviso_model::ModelSkeleton>,
) -> Option<u16> {
    let skeleton = skeleton?;
    let mut weights = BTreeMap::<u16, f32>::new();
    for vertex in triangle {
        for influence in 0..usize::from(vertex.skin_influences) {
            let joint = vertex.joints[influence];
            if usize::from(joint) < skeleton.joints.len() {
                *weights.entry(joint).or_default() += vertex.weights[influence];
            }
        }
    }
    weights
        .into_iter()
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|entry| entry.0)
}

fn joint_lineage(
    skeleton: Option<&newviso_model::ModelSkeleton>,
    joint: Option<u16>,
) -> Vec<String> {
    let Some(skeleton) = skeleton else {
        return Vec::new();
    };
    let mut cursor = joint;
    let mut names = Vec::new();
    while let Some(index) = cursor {
        let Some(joint) = skeleton.joints.get(usize::from(index)) else {
            break;
        };
        names.push(joint.name.clone());
        if names.len() > skeleton.joints.len() {
            break;
        }
        cursor = joint.parent;
    }
    names
}

fn decode_indices(buffer: &newviso_model::IndexBuffer) -> Result<Vec<u32>, String> {
    match buffer.format {
        ModelIndexFormat::U16 => {
            let expected = buffer.index_count as usize * 2;
            if buffer.data.len() < expected {
                return Err(format!(
                    "U16 index buffer has {} bytes, expected at least {}",
                    buffer.data.len(),
                    expected
                ));
            }
            Ok(buffer.data[..expected]
                .chunks_exact(2)
                .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]) as u32)
                .collect())
        }
        ModelIndexFormat::U32 => {
            let expected = buffer.index_count as usize * 4;
            if buffer.data.len() < expected {
                return Err(format!(
                    "U32 index buffer has {} bytes, expected at least {}",
                    buffer.data.len(),
                    expected
                ));
            }
            Ok(buffer.data[..expected]
                .chunks_exact(4)
                .map(|bytes| u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
                .collect())
        }
    }
}

fn read_vec2(stream: &newviso_model::VertexStream, index: usize) -> Result<[f32; 2], String> {
    if stream.format != ModelVertexFormat::Float32x2 {
        return Err(format!(
            "vertex stream {:?} must be Float32x2",
            stream.semantic
        ));
    }
    let stride = usize::try_from(stream.stride).map_err(|_| "vertex stride overflow".to_owned())?;
    let offset = index
        .checked_mul(stride)
        .ok_or_else(|| "vertex stream offset overflow".to_owned())?;
    let end = offset + 8;
    if end > stream.data.len() {
        return Err(format!(
            "vertex {} exceeds {:?} stream bytes={}",
            index,
            stream.semantic,
            stream.data.len()
        ));
    }
    Ok([
        f32::from_le_bytes(stream.data[offset..offset + 4].try_into().unwrap()),
        f32::from_le_bytes(stream.data[offset + 4..offset + 8].try_into().unwrap()),
    ])
}

fn read_vec3(stream: &newviso_model::VertexStream, index: usize) -> Result<Vec3, String> {
    if stream.format != ModelVertexFormat::Float32x3 {
        return Err(format!(
            "vertex stream {:?} must be Float32x3",
            stream.semantic
        ));
    }
    let stride = usize::try_from(stream.stride).map_err(|_| "vertex stride overflow".to_owned())?;
    let offset = index
        .checked_mul(stride)
        .ok_or_else(|| "vertex stream offset overflow".to_owned())?;
    let end = offset + 12;
    if end > stream.data.len() {
        return Err(format!(
            "vertex {} exceeds {:?} stream bytes={}",
            index,
            stream.semantic,
            stream.data.len()
        ));
    }
    Ok(Vec3::new(
        f32::from_le_bytes(stream.data[offset..offset + 4].try_into().unwrap()),
        f32::from_le_bytes(stream.data[offset + 4..offset + 8].try_into().unwrap()),
        f32::from_le_bytes(stream.data[offset + 8..offset + 12].try_into().unwrap()),
    ))
}

fn read_vec4(stream: &newviso_model::VertexStream, index: usize) -> Result<[f32; 4], String> {
    if stream.format != ModelVertexFormat::Float32x4 {
        return Err(format!(
            "vertex stream {:?} must be Float32x4",
            stream.semantic
        ));
    }
    let stride = usize::try_from(stream.stride).map_err(|_| "vertex stride overflow".to_owned())?;
    let offset = index
        .checked_mul(stride)
        .ok_or_else(|| "vertex stream offset overflow".to_owned())?;
    let end = offset + 16;
    if end > stream.data.len() {
        return Err(format!(
            "vertex {} exceeds {:?} stream bytes={}",
            index,
            stream.semantic,
            stream.data.len()
        ));
    }
    Ok([
        f32::from_le_bytes(stream.data[offset..offset + 4].try_into().unwrap()),
        f32::from_le_bytes(stream.data[offset + 4..offset + 8].try_into().unwrap()),
        f32::from_le_bytes(stream.data[offset + 8..offset + 12].try_into().unwrap()),
        f32::from_le_bytes(stream.data[offset + 12..offset + 16].try_into().unwrap()),
    ])
}

fn read_skin_influences(
    vertex: &mut AssetTriangleVertex,
    indices: &newviso_model::VertexStream,
    weights: &newviso_model::VertexStream,
    extra_indices: Option<&newviso_model::VertexStream>,
    extra_weights: Option<&newviso_model::VertexStream>,
    vertex_index: usize,
) -> Result<(), String> {
    let primary_indices = read_vec4(indices, vertex_index)?;
    let primary_weights = read_vec4(weights, vertex_index)?;
    let mut count = 0usize;

    let mut append = |joint_values: [f32; 4], weight_values: [f32; 4]| -> Result<(), String> {
        for lane in 0..4 {
            let joint = joint_values[lane];
            let weight = weight_values[lane];
            if !joint.is_finite()
                || joint < 0.0
                || joint.fract().abs() > 1.0e-4
                || joint > u16::MAX as f32
                || !weight.is_finite()
                || weight < 0.0
            {
                return Err(format!(
                    "invalid skin influence vertex={} lane={} joint={} weight={}",
                    vertex_index, lane, joint, weight
                ));
            }
            if weight > 1.0e-8 && count < 8 {
                vertex.joints[count] = joint as u16;
                vertex.weights[count] = weight;
                count += 1;
            }
        }
        Ok(())
    };
    append(primary_indices, primary_weights)?;
    if let (Some(extra_indices), Some(extra_weights)) = (extra_indices, extra_weights) {
        append(
            read_vec4(extra_indices, vertex_index)?,
            read_vec4(extra_weights, vertex_index)?,
        )?;
    }
    let total = vertex.weights[..count].iter().sum::<f32>();
    if count != 0 && total > 1.0e-8 {
        for weight in &mut vertex.weights[..count] {
            *weight /= total;
        }
        vertex.skin_influences = count as u8;
    }
    Ok(())
}

fn generate_triangle_tangent(triangle: &mut [AssetTriangleVertex; 3]) {
    let edge1 = triangle[1].position.sub(triangle[0].position);
    let edge2 = triangle[2].position.sub(triangle[0].position);
    let duv1 = [
        triangle[1].uv[0] - triangle[0].uv[0],
        triangle[1].uv[1] - triangle[0].uv[1],
    ];
    let duv2 = [
        triangle[2].uv[0] - triangle[0].uv[0],
        triangle[2].uv[1] - triangle[0].uv[1],
    ];
    let det = duv1[0] * duv2[1] - duv1[1] * duv2[0];

    let normal = triangle[0].normal.normalized();
    let (mut tangent, bitangent) = if det.abs() > 1.0e-8 {
        let inv = 1.0 / det;
        (
            Vec3::new(
                (edge1.x * duv2[1] - edge2.x * duv1[1]) * inv,
                (edge1.y * duv2[1] - edge2.y * duv1[1]) * inv,
                (edge1.z * duv2[1] - edge2.z * duv1[1]) * inv,
            ),
            Vec3::new(
                (edge2.x * duv1[0] - edge1.x * duv2[0]) * inv,
                (edge2.y * duv1[0] - edge1.y * duv2[0]) * inv,
                (edge2.z * duv1[0] - edge1.z * duv2[0]) * inv,
            ),
        )
    } else {
        let axis = if normal.y.abs() < 0.999 {
            Vec3::Y
        } else {
            Vec3::new(1.0, 0.0, 0.0)
        };
        let tangent = axis.cross(normal).normalized();
        (tangent, normal.cross(tangent))
    };
    tangent = tangent.sub(normal.mul(tangent.dot(normal))).normalized();
    let handedness = if normal.cross(tangent).dot(bitangent) < 0.0 {
        -1.0
    } else {
        1.0
    };
    for vertex in triangle {
        vertex.tangent = [tangent.x, tangent.y, tangent.z, handedness];
    }
}

pub(super) fn transformed_local_bounds(
    local_bounds: SceneBounds,
    transform: SceneTransform,
) -> Result<SceneBounds, String> {
    if [
        local_bounds.min.x,
        local_bounds.min.y,
        local_bounds.min.z,
        local_bounds.max.x,
        local_bounds.max.y,
        local_bounds.max.z,
    ]
    .iter()
    .any(|value| !value.is_finite())
    {
        return Err("asset model has non-finite local bounds".to_owned());
    }

    let min = local_bounds.min;
    let max = local_bounds.max;
    let mut world_min = Vec3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY);
    let mut world_max = Vec3::new(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);

    for x in [min.x, max.x] {
        for y in [min.y, max.y] {
            for z in [min.z, max.z] {
                let p = transform_point(
                    Vec3::new(x, y, z),
                    transform.scale,
                    transform.rotation_degrees,
                    transform.position,
                );
                world_min.x = world_min.x.min(p.x);
                world_min.y = world_min.y.min(p.y);
                world_min.z = world_min.z.min(p.z);
                world_max.x = world_max.x.max(p.x);
                world_max.y = world_max.y.max(p.y);
                world_max.z = world_max.z.max(p.z);
            }
        }
    }

    Ok(SceneBounds {
        min: world_min,
        max: world_max,
    })
}

fn transformed_model_bounds(
    model: &ModelResource,
    transform: SceneTransform,
) -> Result<SceneBounds, String> {
    if !model.bounds.is_finite() {
        return Err(format!("model '{}' has non-finite bounds", model.name));
    }
    transformed_local_bounds(
        SceneBounds {
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
        transform,
    )
}

#[cfg(test)]
mod dent_tests {
    use super::*;
    #[test]
    fn local_dent_deforms_contact_region_and_preserves_remote_geometry() {
        let dent = SceneModelDent {
            point: [0.0; 3],
            displacement: [0.0, -0.2, 0.0],
            radius: 1.0,
        };
        let (point, normal, tangent) = apply_fragment_dents(
            Vec3::ZERO,
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            &[dent.clone()],
        );
        assert!((point.y + 0.2).abs() < 1.0e-6);
        assert!(normal.y > 0.99 && tangent.x > 0.99);
        let (remote, _, _) =
            apply_fragment_dents(Vec3::new(2.0, 0.0, 0.0), normal, tangent, &[dent]);
        assert!((remote.x - 2.0).abs() < 1.0e-6 && remote.y == 0.0);
    }
}

#[cfg(test)]
mod joint_partition_tests {
    use super::*;
    use newviso_model::{
        Bounds3, IndexBuffer, MeshPrimitive, MeshResource, ModelJoint, ModelSkeleton, VertexStream,
    };

    fn stream(semantic: VertexSemantic, width: usize, values: &[f32]) -> VertexStream {
        VertexStream {
            semantic,
            format: match width {
                3 => ModelVertexFormat::Float32x3,
                _ => ModelVertexFormat::Float32x4,
            },
            stride: (width * 4) as u32,
            vertex_count: 6,
            data: Arc::from(
                values
                    .iter()
                    .flat_map(|v| v.to_le_bytes())
                    .collect::<Vec<_>>(),
            ),
        }
    }

    #[test]
    fn mixed_head_and_hand_mesh_keeps_every_triangle_and_material() {
        let bounds = Bounds3 {
            min: [-1.0; 3],
            max: [1.0; 3],
        };
        let joint = |name: &str, parent: Option<u16>| ModelJoint {
            name: name.into(),
            tag: 0,
            parent,
            inverse_bind_matrix: identity_matrix(),
            bind_translation: [0.0; 3],
            bind_rotation: [0.0, 0.0, 0.0, 1.0],
            bind_scale: [1.0; 3],
        };
        let skeleton = ModelSkeleton {
            name: "test".into(),
            joints: vec![
                joint("root", None),
                joint("neck", Some(0)),
                joint("eye", Some(1)),
                joint("forearm", Some(0)),
                joint("hand", Some(3)),
            ],
        };
        let indices = [0u32, 1, 2, 3, 4, 5];
        let influence: Vec<f32> = [2.0, 2.0, 2.0, 4.0, 4.0, 4.0]
            .into_iter()
            .flat_map(|j| [j, 0.0, 0.0, 0.0])
            .collect();
        let weights: Vec<f32> = (0..6).flat_map(|_| [1.0, 0.0, 0.0, 0.0]).collect();
        let mesh = MeshResource {
            name: "shared_skin".into(),
            bounds,
            vertex_streams: vec![
                stream(
                    VertexSemantic::Position,
                    3,
                    &[
                        0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 1.0,
                        0.0, 1.0, 1.0,
                    ],
                ),
                stream(VertexSemantic::JointIndices, 4, &influence),
                stream(VertexSemantic::JointWeights, 4, &weights),
            ],
            index_buffer: IndexBuffer {
                format: ModelIndexFormat::U32,
                index_count: 6,
                data: Arc::from(
                    indices
                        .iter()
                        .flat_map(|i| i.to_le_bytes())
                        .collect::<Vec<_>>(),
                ),
            },
            primitives: vec![MeshPrimitive {
                first_index: 0,
                index_count: 6,
                base_vertex: 0,
                material_slot: Some(7),
            }],
        };
        let model = ModelResource {
            id: AssetId(1),
            name: "mixed".into(),
            bounds,
            meshes: vec![mesh],
            material_slots: vec![],
            skin_source_to_model: identity_matrix(),
            skeleton: Some(skeleton),
            animations: vec![],
            fragment: None,
        };
        let (vertices, ranges) = expand_model_triangles(&model).unwrap();
        assert_eq!(vertices.len(), 6);
        assert_eq!(ranges.len(), 2);
        assert!(ranges
            .iter()
            .all(|range| range.mesh_name.as_ref() == "shared_skin"
                && range.material_slot == Some(7)
                && range.vertex_count == 3));
        assert!(ranges[0].joint_lineage.iter().any(|j| j == "neck"));
        assert!(ranges[1].joint_lineage.iter().any(|j| j == "forearm"));
        assert!(!ranges[1].joint_lineage.iter().any(|j| j == "neck"));
    }
}

#[cfg(test)]
mod fragment_surface_tests {
    use super::*;
    #[test]
    fn glass_triangle_hit_keeps_the_hit_uv_instead_of_box_overlap() {
        let (hit, barycentric) = closest_fragment_triangle(
            [0.25, 0.25, 0.1],
            [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        )
        .unwrap();
        assert_eq!(hit, [0.25, 0.25, 0.0]);
        assert_eq!(barycentric, [0.5, 0.25, 0.25]);
    }
    #[test]
    fn collapsed_broken_glass_is_not_a_damage_surface() {
        assert!(closest_fragment_triangle([0.0; 3], [[0.0; 3]; 3]).is_none());
    }
}
