use super::*;

const BIND_FLOATS: usize = FLOATS_PER_VERTEX + 19;

pub(super) fn gpu_skinning_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("NEWVISO_CPU_SKINNING").as_deref() != Ok("1"))
}

#[derive(Debug)]
struct SkinModel {
    source_vertices: std::sync::Arc<[AssetTriangleVertex]>,
    buffer: u32,
    byte_len: u64,
    vertices: usize,
    unique_vertices: u32,
    linear: bool,
    joints: usize,
}
impl SkinModel {
    fn matches_source(&self, vertices: &std::sync::Arc<[AssetTriangleVertex]>) -> bool {
        std::sync::Arc::ptr_eq(&self.source_vertices, vertices)
    }
}

#[derive(Debug)]
struct SkinEntity {
    source: u64,
    uniforms: [u32; SCENE_FRAME_SLOTS],
    palettes: [u32; SCENE_FRAME_SLOTS],
    groups: [u32; SCENE_FRAME_SLOTS],
    output_buffers: [u32; SCENE_FRAME_SLOTS],
}
impl SkinEntity {
    fn destroy(self, render: &RenderClient) {
        for group in self.groups {
            if group != 0 {
                render.destroy_bind_group(group);
            }
        }
        for buffer in self.uniforms.into_iter().chain(self.palettes) {
            if buffer != 0 {
                render.destroy_buffer(buffer);
            }
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct SkinningGpu {
    shader: u32,
    layout: u32,
    pipeline: u32,
    models: BTreeMap<u64, SkinModel>,
    entities: BTreeMap<u64, SkinEntity>,
    palette_scratch: Vec<f32>,
}
impl SkinningGpu {
    fn prepare_model(
        &mut self,
        render: &RenderClient,
        source: u64,
        vertices: &std::sync::Arc<[AssetTriangleVertex]>,
        joints: usize,
    ) -> Result<(), String> {
        if self
            .models
            .get(&source)
            .is_some_and(|model| model.matches_source(vertices))
        {
            return Ok(());
        }
        let packed = pack_bind_vertices(vertices, joints)?;
        let byte_len = packed.data.len() as u64 * 4;
        let buffer = render.create_buffer(
            "newviso.scene.skinning.bind",
            byte_len.max(4),
            "Storage",
            "GpuOnly",
        )?;
        if let Err(error) = render.write_buffer_f32(buffer, 0, &packed.data) {
            render.destroy_buffer(buffer);
            return Err(error);
        }
        host_runtime::debug(
            "newviso.scene.skinning",
            format!(
                "source={source} expanded_vertices={} deformation_vertices={} linear={} joints={joints}",
                vertices.len(), packed.unique_vertices, packed.linear
            ),
        );
        // Replacing the same logical asset invalidates every descriptor that
        // referenced the previous immutable bind buffer, even if dimensions match.
        let stale: Vec<_> = self
            .entities
            .iter()
            .filter_map(|(id, entity)| (entity.source == source).then_some(*id))
            .collect();
        for id in stale {
            self.entities.remove(&id).unwrap().destroy(render);
        }
        if let Some(old) = self.models.remove(&source) {
            render.destroy_buffer(old.buffer);
        }
        self.models.insert(
            source,
            SkinModel {
                source_vertices: std::sync::Arc::clone(vertices),
                buffer,
                byte_len,
                vertices: vertices.len(),
                unique_vertices: packed.unique_vertices,
                linear: packed.linear,
                joints,
            },
        );
        Ok(())
    }

    fn destroy(self, render: &RenderClient) {
        for entity in self.entities.into_values() {
            entity.destroy(render);
        }
        for model in self.models.into_values() {
            render.destroy_buffer(model.buffer);
        }
        if self.pipeline != 0 {
            render.destroy_pipeline(self.pipeline);
        }
        if self.shader != 0 {
            render.destroy_shader(self.shader);
        }
        if self.layout != 0 {
            render.destroy_bind_group_layout(self.layout);
        }
    }
}

struct PackedBindGeometry {
    data: Vec<f32>,
    unique_vertices: u32,
    linear: bool,
}

fn pack_bind_vertices(
    vertices: &[AssetTriangleVertex],
    joints: usize,
) -> Result<PackedBindGeometry, String> {
    // Expanded triangle corners often repeat the same authored vertex. Deform
    // each exact attribute/influence tuple once, then scatter it to every
    // original corner. Material ranges, indices, UV seams and shadow geometry
    // retain their existing ABI and full detail.
    let mut lookup = std::collections::HashMap::<[u32; 34], usize>::new();
    let mut rows = Vec::<[f32; 34]>::new();
    let mut duplicates = Vec::<Vec<u32>>::new();
    let mut scratch = Vec::with_capacity(34);
    for (index, vertex) in vertices.iter().enumerate() {
        let count = vertex.skin_influences as usize;
        if count > vertex.joints.len()
            || vertex.joints[..count]
                .iter()
                .any(|joint| *joint as usize >= joints)
            || vertex.weights[..count]
                .iter()
                .any(|weight| !weight.is_finite())
        {
            return Err("GPU skin input has invalid influence count, joint or weight".into());
        }
        scratch.clear();
        geometry::append_local_asset_vertex(
            &mut scratch,
            vertex.position,
            vertex.normal,
            vertex.tangent,
            vertex.color,
            vertex.uv,
        );
        scratch.extend(vertex.joints.map(f32::from));
        scratch.extend(vertex.weights);
        scratch.push(count as f32);
        let row: [f32; 34] = scratch.as_slice().try_into().unwrap();
        let key = row.map(f32::to_bits);
        let unique = *lookup.entry(key).or_insert_with(|| {
            let unique = rows.len();
            rows.push(row);
            duplicates.push(Vec::new());
            unique
        });
        duplicates[unique].push(
            u32::try_from(index).map_err(|_| "GPU skin expanded index exceeds u32".to_owned())?,
        );
    }
    let unique_vertices =
        u32::try_from(rows.len()).map_err(|_| "GPU skin unique count exceeds u32".to_owned())?;
    if rows.len().saturating_mul(10) > vertices.len().saturating_mul(9) {
        // A mostly unique mesh gains little from scatter indirection. Retain
        // linear, coalesced output writes and exactly the same full geometry.
        let size = vertices
            .len()
            .checked_mul(BIND_FLOATS)
            .ok_or_else(|| "GPU skin linear bind size overflow".to_owned())?;
        let mut data = vec![0.0; size];
        for (row, indices) in rows.into_iter().zip(duplicates) {
            for index in indices {
                let begin = index as usize * BIND_FLOATS;
                data[begin..begin + 34].copy_from_slice(&row);
            }
        }
        return Ok(PackedBindGeometry {
            data,
            unique_vertices: u32::try_from(vertices.len())
                .map_err(|_| "GPU skin linear count exceeds u32".to_owned())?,
            linear: true,
        });
    }
    let mut offset = rows
        .len()
        .checked_mul(BIND_FLOATS)
        .ok_or_else(|| "GPU skin bind size overflow".to_owned())?;
    let mut data = Vec::with_capacity(offset.saturating_add(vertices.len()));
    for (row, indices) in rows.into_iter().zip(&duplicates) {
        data.extend(row);
        data.push(f32::from_bits(
            u32::try_from(offset).map_err(|_| "GPU skin scatter offset exceeds u32".to_owned())?,
        ));
        data.push(f32::from_bits(indices.len() as u32));
        offset += indices.len();
    }
    for indices in duplicates {
        data.extend(indices.into_iter().map(f32::from_bits));
    }
    Ok(PackedBindGeometry {
        data,
        unique_vertices,
        linear: false,
    })
}

impl Scene3dRuntime {
    pub(super) fn initialize_gpu_skinning(&mut self, render: &RenderClient) -> Result<(), String> {
        if !gpu_skinning_enabled() {
            return Ok(());
        }
        let mut state = SkinningGpu::default();
        let result = (|| {
            state.shader = render.create_shader_spirv(
                "newviso.scene.skinning",
                ShaderStage::Compute,
                include_bytes!("../assets/skinning.comp.spv"),
            )?;
            state.layout = render.create_bind_group_layout(
                "newviso.scene.skinning.bindings",
                &[
                    "UniformBuffer",
                    "StorageBuffer",
                    "StorageBuffer",
                    "StorageBuffer",
                ],
            )?;
            state.pipeline = render.create_compute_pipeline(
                "newviso.scene.skinning",
                state.shader,
                &[state.layout],
                "newviso.scene.skinning.v1",
            )?;
            Ok(())
        })();
        if let Err(error) = result {
            state.destroy(render);
            return Err(error);
        }
        self.gpu_skinning = Some(state);
        Ok(())
    }

    pub(super) fn shutdown_gpu_skinning(&mut self, render: &RenderClient) {
        if let Some(state) = self.gpu_skinning.take() {
            state.destroy(render);
        }
    }

    pub(super) fn warmup_gpu_skinning_models(&mut self) -> Result<(), String> {
        let Some(state) = self.gpu_skinning.as_mut() else {
            return Ok(());
        };
        let render = RenderClient::new();
        for (id, animation) in &self.skinned_entities {
            let Some(mesh) = self.asset_meshes.get(id) else {
                continue;
            };
            let source = mesh.source_model_id.0;
            let Some(vertices) = self.asset_model_cache.get(&source) else {
                continue;
            };
            state.prepare_model(
                &render,
                source,
                vertices,
                animation.presented_skin_palette().len(),
            )?;
        }
        Ok(())
    }

    /// Run before shadow/depth/water recording, rather than leaving deformation
    /// in the later main-view graph. All consumers see the same acquired slot.
    pub(super) fn dispatch_gpu_skinning(
        &mut self,
        render: &RenderClient,
        slot: usize,
        gpu: GpuScene,
    ) -> Result<(), String> {
        let state = self
            .gpu_skinning
            .as_mut()
            .ok_or_else(|| "GPU skinning was not initialized".to_owned())?;
        let obsolete: Vec<_> = state
            .entities
            .keys()
            .copied()
            .filter(|id| {
                !self.skinned_entities.contains_key(id) || !self.asset_meshes.contains_key(id)
            })
            .collect();
        for id in obsolete {
            state.entities.remove(&id).unwrap().destroy(render);
        }

        let mut dispatches = Vec::new();
        // Include resident offscreen skeletons: their shadows and reflection
        // passes can still be visible even when their main view was culled.
        for (id, animation) in &self.skinned_entities {
            let Some(mesh) = self.asset_meshes.get(id) else {
                continue;
            };
            let Some(first) = mesh.skinned_first_vertex else {
                continue;
            };
            let palette = animation.presented_skin_palette();
            let source = mesh.source_model_id.0;
            let vertices = self
                .asset_model_cache
                .get(&source)
                .ok_or_else(|| format!("GPU skin source {source} is not resident"))?;
            state.prepare_model(render, source, vertices, palette.len())?;
            let model = &state.models[&source];
            if model.vertices != mesh.vertex_count as usize || model.joints != palette.len() {
                return Err(format!("GPU skin source dimensions changed entity={id}"));
            }
            let stale = state.entities.get(id).is_some_and(|entity| {
                entity.source != source || entity.output_buffers != gpu.skinned_vertex_buffers
            });
            if stale {
                state.entities.remove(id).unwrap().destroy(render);
            }
            if !state.entities.contains_key(id) {
                let mut entity = SkinEntity {
                    source,
                    uniforms: [0; SCENE_FRAME_SLOTS],
                    palettes: [0; SCENE_FRAME_SLOTS],
                    groups: [0; SCENE_FRAME_SLOTS],
                    output_buffers: gpu.skinned_vertex_buffers,
                };
                let allocation = (|| {
                    for frame_slot in 0..SCENE_FRAME_SLOTS {
                        entity.uniforms[frame_slot] = render.create_frame_buffer(
                            frame_slot,
                            "newviso.scene.skinning.frame",
                            16,
                            "Uniform",
                            "CpuToGpu",
                        )?;
                        entity.palettes[frame_slot] = render.create_frame_buffer(
                            frame_slot,
                            "newviso.scene.skinning.palette",
                            (palette.len() as u64 * 64).max(64),
                            "Storage",
                            "CpuToGpu",
                        )?;
                        entity.groups[frame_slot] = render.create_compute_bind_group(
                            "newviso.scene.skinning.group",
                            state.layout,
                            (entity.uniforms[frame_slot], 0, 16),
                            [
                                (model.buffer, 0, model.byte_len.max(4)),
                                (
                                    entity.palettes[frame_slot],
                                    0,
                                    (palette.len() as u64 * 64).max(64),
                                ),
                                (
                                    gpu.skinned_vertex_buffers[frame_slot],
                                    0,
                                    gpu.skinned_vertex_capacity as u64 * VERTEX_STRIDE,
                                ),
                            ],
                        )?;
                    }
                    Ok(())
                })();
                if let Err(error) = allocation {
                    entity.destroy(render);
                    return Err(error);
                }
                state.entities.insert(*id, entity);
            }
            let entity = &state.entities[id];
            let dimensions = [
                model.unique_vertices,
                first,
                palette.len() as u32,
                u32::from(model.linear),
            ];
            let bytes: Vec<u8> = dimensions.into_iter().flat_map(u32::to_le_bytes).collect();
            render.write_buffer(entity.uniforms[slot], 0, &bytes)?;
            state.palette_scratch.clear();
            state
                .palette_scratch
                .extend(palette.iter().flatten().copied());
            render.write_buffer_f32(entity.palettes[slot], 0, &state.palette_scratch)?;
            dispatches.push((entity.groups[slot], model.unique_vertices.div_ceil(128)));
        }
        let used: BTreeSet<_> = state
            .entities
            .values()
            .map(|entity| entity.source)
            .collect();
        let unused: Vec<_> = state
            .models
            .keys()
            .filter(|source| !used.contains(source))
            .copied()
            .collect();
        for source in unused {
            render.destroy_buffer(state.models.remove(&source).unwrap().buffer);
        }
        if dispatches.is_empty() {
            return Ok(());
        }
        render.record_unit_batch(|record| {
            record.set_draw_list_kind(None)?;
            record.set_render_phase(Some("VisibilityCull"))?;
            record.set_pipeline(state.pipeline)?;
            for (group, count) in dispatches {
                record.set_bind_group(0, group)?;
                record.dispatch(count, 1, 1)?;
            }
            record.set_render_phase(None)
        })?;
        let input = RenderGraphResourceId(1);
        let output = RenderGraphResourceId(2);
        let mut pass = RenderGraphPassDesc::new(
            RenderGraphPassId(1),
            "newviso.skinning.deform",
            RenderGraphPassKind::VisibilityCull,
        )
        .with_domain(RenderGraphPassDomain::Render3d)
        .reads(input, RenderGraphResourceUsage::StorageBuffer)
        .writes(output, RenderGraphResourceUsage::StorageBuffer);
        pass.queue = RenderGraphQueueKind::Compute;
        let mut graph = RenderGraphDesc::new("newviso.skinning")
            .add_resource(RenderGraphResourceDesc::external(
                input,
                "newviso.skinning.bind_palette",
                RenderGraphResourceUsage::StorageBuffer,
            ))
            .add_resource(RenderGraphResourceDesc::external(
                output,
                "newviso.skinning.vertices",
                RenderGraphResourceUsage::StorageBuffer,
            ))
            .add_pass(pass);
        graph.frame_index = self.frame_index;
        render.submit_render_graph(graph)?;
        // CPU snapshots remain immutable bind data in this mode.
        for ranges in &mut self.skinned_dirty_ranges {
            ranges.clear();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn same_sized_source_replacement_cannot_reuse_stale_bind_geometry() {
        let vertex = AssetTriangleVertex {
            position: Vec3::ZERO,
            normal: Vec3::new(0., 1., 0.),
            tangent: [1., 0., 0., 1.],
            color: [1.; 4],
            uv: [0.; 2],
            joints: [0; 8],
            weights: [0.; 8],
            skin_influences: 0,
        };
        let source: std::sync::Arc<[AssetTriangleVertex]> = std::sync::Arc::from([vertex]);
        let model = SkinModel {
            source_vertices: std::sync::Arc::clone(&source),
            buffer: 1,
            byte_len: 144,
            vertices: 1,
            unique_vertices: 1,
            linear: true,
            joints: 1,
        };
        assert!(model.matches_source(&std::sync::Arc::clone(&source)));
        assert!(!model.matches_source(&std::sync::Arc::from([vertex])));
        let mut changed = vertex;
        changed.position.x = 2.;
        assert!(!model.matches_source(&std::sync::Arc::from([changed])));
    }

    #[test]
    fn mostly_unique_mesh_retains_linear_corner_order() {
        let vertex = AssetTriangleVertex {
            position: Vec3::ZERO,
            normal: Vec3::new(0., 1., 0.),
            tangent: [1., 0., 0., 1.],
            color: [1.; 4],
            uv: [0.; 2],
            joints: [0; 8],
            weights: [0.; 8],
            skin_influences: 0,
        };
        let mut second = vertex;
        second.uv = [0.75, 0.5];
        let packed = pack_bind_vertices(&[vertex, second], 1).unwrap();
        assert!(packed.linear);
        assert_eq!(packed.unique_vertices, 2);
        assert_eq!(packed.data.len(), 2 * BIND_FLOATS);
        assert_eq!(&packed.data[BIND_FLOATS + 11..BIND_FLOATS + 13], &second.uv);
    }

    #[test]
    fn identical_corners_share_deformation_but_uv_seams_stay_distinct() {
        let vertex = AssetTriangleVertex {
            position: Vec3::ZERO,
            normal: Vec3::new(0., 1., 0.),
            tangent: [1., 0., 0., 1.],
            color: [1.; 4],
            uv: [0.; 2],
            joints: [0; 8],
            weights: [0.; 8],
            skin_influences: 0,
        };
        let mut seam = vertex;
        seam.uv[0] = 1.;
        let packed = pack_bind_vertices(&[vertex, vertex, seam], 1).unwrap();
        assert_eq!(packed.unique_vertices, 2);
        assert_eq!(packed.data[34].to_bits(), 72);
        assert_eq!(packed.data[35].to_bits(), 2);
        assert_eq!(packed.data[70].to_bits(), 74);
        assert_eq!(packed.data[71].to_bits(), 1);
        assert_eq!(
            packed.data[72..]
                .iter()
                .map(|x| x.to_bits())
                .collect::<Vec<_>>(),
            [0, 1, 2]
        );
    }

    #[test]
    fn invalid_gpu_influences_are_rejected_before_dispatch() {
        let mut vertex = AssetTriangleVertex {
            position: Vec3::ZERO,
            normal: Vec3::new(0., 1., 0.),
            tangent: [1., 0., 0., 1.],
            color: [1.; 4],
            uv: [0.; 2],
            joints: [0; 8],
            weights: [0.; 8],
            skin_influences: 1,
        };
        vertex.weights[0] = 1.;
        let packed = pack_bind_vertices(&[vertex], 1).unwrap();
        assert_eq!(packed.unique_vertices, 1);
        let data = packed.data;
        assert!(packed.linear);
        assert_eq!(data.len(), BIND_FLOATS);
        assert_eq!(&data[17..25], &[0.; 8]);
        assert_eq!(data[25], 1.);
        assert_eq!(data[33], 1.);
        vertex.joints[0] = 1;
        assert!(pack_bind_vertices(&[vertex], 1).is_err());
        vertex.joints[0] = 0;
        vertex.weights[0] = f32::NAN;
        assert!(pack_bind_vertices(&[vertex], 1).is_err());
        vertex.weights[0] = 1.;
        vertex.skin_influences = 9;
        assert!(pack_bind_vertices(&[vertex], 1).is_err());
    }
}
