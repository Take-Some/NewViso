use super::*;

pub(super) fn instance_cell(position: Vec3) -> (i32, i32, i32) {
    let q = |value: f32| (value / ASSET_INSTANCE_CELL_SIZE).floor() as i32;
    (q(position.x), q(position.y), q(position.z))
}

pub(super) fn instance_ids_sphere(world: &SceneWorld, stable_ids: &[u64]) -> Option<[f32; 4]> {
    let mut min = Vec3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY);
    let mut max = Vec3::new(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
    let mut any = false;
    for stable_id in stable_ids {
        let entity = world.entity(SceneEntityId(*stable_id))?;
        min.x = min.x.min(entity.bounds.min.x);
        min.y = min.y.min(entity.bounds.min.y);
        min.z = min.z.min(entity.bounds.min.z);
        max.x = max.x.max(entity.bounds.max.x);
        max.y = max.y.max(entity.bounds.max.y);
        max.z = max.z.max(entity.bounds.max.z);
        any = true;
    }
    if !any {
        return None;
    }
    let center = Vec3::new(
        (min.x + max.x) * 0.5,
        (min.y + max.y) * 0.5,
        (min.z + max.z) * 0.5,
    );
    let radius = max.sub(center).length().max(0.05);
    Some([center.x, center.y, center.z, radius])
}

pub(super) fn asset_mesh_vertex_binding(
    mesh: &CpuAssetMesh,
    gpu: GpuScene,
    frame_slot: usize,
) -> Result<(u32, i32), String> {
    let Some(skinned_first_vertex) = mesh.skinned_first_vertex else {
        return Ok((gpu.asset_vertex_buffer, 0));
    };
    let delta = i64::from(skinned_first_vertex) - i64::from(mesh.first_vertex);
    let vertex_offset = i32::try_from(delta).map_err(|_| {
        format!(
            "skinned vertex offset exceeds i32 compact_first={} global_first={}",
            skinned_first_vertex, mesh.first_vertex
        )
    })?;
    Ok((gpu.skinned_vertex_buffers[frame_slot], vertex_offset))
}

#[derive(Clone, Copy, Debug)]
pub(super) struct DirectAssetDraw {
    pub(super) material_group: u32,
    pub(super) vertex_buffer: u32,
    pub(super) vertex_offset: i32,
    pub(super) index_count: u32,
    pub(super) first_index: u32,
    pub(super) instance_count: u32,
    pub(super) first_instance: u32,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct AlphaAssetDraw {
    pub(super) distance_sq: f32,
    pub(super) material_group: u32,
    pub(super) vertex_buffer: u32,
    pub(super) vertex_offset: i32,
    pub(super) first_index: u32,
    pub(super) index_count: u32,
    pub(super) instance_index: u32,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct PendingHizAssetDraw {
    pub(super) material_group: u32,
    pub(super) sphere: [f32; 4],
    pub(super) index_count: u32,
    pub(super) first_index: u32,
    pub(super) instance_count: u32,
    pub(super) first_instance: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct VisibleBatchRun {
    pub(super) first_member: usize,
    pub(super) member_count: usize,
}

pub(super) fn visible_batch_runs<'a>(
    batch: &'a GpuResidentInstanceBatch,
    visible_slots: &'a BTreeMap<u64, u32>,
) -> impl Iterator<Item = VisibleBatchRun> + 'a {
    let mut cursor = 0;
    std::iter::from_fn(move || {
        while cursor < batch.stable_ids.len()
            && !visible_slots.contains_key(&batch.stable_ids[cursor])
        {
            cursor += 1;
        }
        let first_member = cursor;
        while let Some(slot) = batch
            .stable_ids
            .get(cursor)
            .and_then(|id| visible_slots.get(id))
        {
            debug_assert_eq!(
                *slot,
                batch.first_instance.saturating_add(cursor as u32),
                "persistent instance batch slots must remain contiguous"
            );
            cursor += 1;
        }
        (cursor > first_member).then_some(VisibleBatchRun {
            first_member,
            member_count: cursor - first_member,
        })
    })
}

/// Contiguous native indirect ranges with the same material. Offset gaps must
/// start a new run; every pass consumes exactly the same visible draw stream.
pub(super) fn material_indirect_runs(
    draws: &[(u32, u64)],
) -> impl Iterator<Item = (u32, u64, usize)> + '_ {
    let mut cursor = 0;
    std::iter::from_fn(move || {
        let &(material, offset) = draws.get(cursor)?;
        let start = cursor;
        cursor += 1;
        while cursor < draws.len()
            && draws[cursor].0 == material
            && draws[cursor].1 == offset + (cursor - start) as u64 * HIZ_INDIRECT_STRIDE
        {
            cursor += 1;
        }
        Some((material, offset, cursor - start))
    })
}

pub(super) fn draw_material_indirect_runs(
    render: &RenderClient,
    buffer: u32,
    draws: &[(u32, u64)],
) -> Result<(), String> {
    for (material, offset, count) in material_indirect_runs(draws) {
        render.set_bind_group(1, material)?;
        render.draw_indexed_indirect(
            buffer,
            offset,
            u32::try_from(count).map_err(|_| "material indirect count exceeds u32".to_owned())?,
            HIZ_INDIRECT_STRIDE as u32,
        )?;
    }
    Ok(())
}

pub(super) fn append_hiz_candidate(out: &mut Vec<u8>, sphere: [f32; 4]) {
    for value in sphere {
        out.extend_from_slice(&value.to_le_bytes());
    }
    for value in [1u32, 0, 0, 0] {
        out.extend_from_slice(&value.to_le_bytes());
    }
}

pub(super) fn append_indexed_indirect_command(
    out: &mut Vec<u8>,
    index_count: u32,
    instance_count: u32,
    first_index: u32,
    first_instance: u32,
) {
    out.extend_from_slice(&index_count.to_le_bytes());
    out.extend_from_slice(&instance_count.to_le_bytes());
    out.extend_from_slice(&first_index.to_le_bytes());
    out.extend_from_slice(&0i32.to_le_bytes());
    out.extend_from_slice(&first_instance.to_le_bytes());
}

pub(super) struct MaterialVertexBindings {
    material: Option<u32>,
    vertex_buffer: Option<u32>,
}
impl MaterialVertexBindings {
    pub(super) fn new(material: Option<u32>, vertex_buffer: Option<u32>) -> Self {
        Self {
            material,
            vertex_buffer,
        }
    }
    pub(super) fn bind(
        &mut self,
        render: &RenderClient,
        material: u32,
        vertex_buffer: u32,
    ) -> Result<(), String> {
        self.bind_material(render, material)?;
        self.bind_vertex(render, vertex_buffer)
    }
    pub(super) fn bind_material(
        &mut self,
        render: &RenderClient,
        material: u32,
    ) -> Result<(), String> {
        if self.material != Some(material) {
            render.set_bind_group(1, material)?;
            self.material = Some(material);
        }
        Ok(())
    }
    pub(super) fn bind_vertex(
        &mut self,
        render: &RenderClient,
        vertex_buffer: u32,
    ) -> Result<(), String> {
        if self.vertex_buffer != Some(vertex_buffer) {
            render.set_vertex_buffer(0, vertex_buffer, 0)?;
            self.vertex_buffer = Some(vertex_buffer);
        }
        Ok(())
    }
}
