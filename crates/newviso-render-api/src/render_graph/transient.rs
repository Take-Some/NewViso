use std::collections::BTreeMap;

use crate::{Extent2D, TextureFormat};
use serde::{Deserialize, Serialize};

use super::{
    RenderGraphDesc, RenderGraphResourceDesc, RenderGraphResourceId, RenderGraphResourceLifetime,
    RenderGraphResourceLifetimeReport, RenderGraphResourceUsage, ResourceLifetimeInterval,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceClass {
    Texture,
    Buffer,
}

/// Exact logical usage requirements for a transient allocation.
///
/// This is intentionally a bit-set rather than a single enum variant: a render-graph
/// texture is commonly written as an attachment/storage image and sampled later in
/// the same live interval. Phase 4 only aliases resources whose full usage sets are
/// equal, which is conservative but keeps backend allocation requirements explicit.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceUsageClass {
    pub bits: u16,
}

impl ResourceUsageClass {
    pub const COLOR_ATTACHMENT: u16 = 1 << 0;
    pub const DEPTH_ATTACHMENT: u16 = 1 << 1;
    pub const SAMPLED_TEXTURE: u16 = 1 << 2;
    pub const STORAGE_TEXTURE: u16 = 1 << 3;
    pub const VERTEX_BUFFER: u16 = 1 << 4;
    pub const INDEX_BUFFER: u16 = 1 << 5;
    pub const UNIFORM_BUFFER: u16 = 1 << 6;
    pub const STORAGE_BUFFER: u16 = 1 << 7;

    const TEXTURE_MASK: u16 = Self::COLOR_ATTACHMENT
        | Self::DEPTH_ATTACHMENT
        | Self::SAMPLED_TEXTURE
        | Self::STORAGE_TEXTURE;
    const BUFFER_MASK: u16 =
        Self::VERTEX_BUFFER | Self::INDEX_BUFFER | Self::UNIFORM_BUFFER | Self::STORAGE_BUFFER;

    #[inline]
    pub const fn contains_bits(self, bits: u16) -> bool {
        self.bits & bits == bits
    }

    #[inline]
    pub fn insert(&mut self, usage: RenderGraphResourceUsage) {
        self.bits |= usage_bit(usage);
    }

    #[inline]
    pub fn from_resource_and_lifetime(
        resource: &RenderGraphResourceDesc,
        lifetime: &ResourceLifetimeInterval,
    ) -> Self {
        let mut class = Self::default();
        class.insert(resource.usage);
        for event in &lifetime.history {
            if let Some(usage) = event.usage {
                class.insert(usage);
            }
        }
        class
    }

    #[inline]
    pub const fn resource_class(self) -> Option<ResourceClass> {
        let texture = self.bits & Self::TEXTURE_MASK != 0;
        let buffer = self.bits & Self::BUFFER_MASK != 0;
        match (texture, buffer) {
            (true, false) => Some(ResourceClass::Texture),
            (false, true) => Some(ResourceClass::Buffer),
            _ => None,
        }
    }
}

#[inline]
const fn usage_bit(usage: RenderGraphResourceUsage) -> u16 {
    match usage {
        RenderGraphResourceUsage::ColorAttachment => ResourceUsageClass::COLOR_ATTACHMENT,
        RenderGraphResourceUsage::DepthAttachment => ResourceUsageClass::DEPTH_ATTACHMENT,
        RenderGraphResourceUsage::DepthAttachmentSampled => {
            ResourceUsageClass::DEPTH_ATTACHMENT | ResourceUsageClass::SAMPLED_TEXTURE
        }
        RenderGraphResourceUsage::SampledTexture => ResourceUsageClass::SAMPLED_TEXTURE,
        RenderGraphResourceUsage::StorageTexture => ResourceUsageClass::STORAGE_TEXTURE,
        RenderGraphResourceUsage::VertexBuffer => ResourceUsageClass::VERTEX_BUFFER,
        RenderGraphResourceUsage::IndexBuffer => ResourceUsageClass::INDEX_BUFFER,
        RenderGraphResourceUsage::UniformBuffer => ResourceUsageClass::UNIFORM_BUFFER,
        RenderGraphResourceUsage::StorageBuffer => ResourceUsageClass::STORAGE_BUFFER,
    }
}

/// Provider-neutral physical compatibility key for a transient logical resource.
///
/// Equal keys mean the resources have the same allocation shape and capability
/// requirements. Equal keys alone do not permit aliasing; their live intervals must
/// also be disjoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransientResourceCompatibilityKey {
    pub resource_class: ResourceClass,
    #[serde(default)]
    pub format: Option<TextureFormat>,
    #[serde(default)]
    pub extent: Option<Extent2D>,
    #[serde(default)]
    pub byte_size: Option<u64>,
    pub sample_count: u8,
    pub usage_class: ResourceUsageClass,
}

impl TransientResourceCompatibilityKey {
    pub fn from_resource(
        resource: &RenderGraphResourceDesc,
        lifetime: &ResourceLifetimeInterval,
    ) -> Option<Self> {
        if !matches!(
            resource.lifetime,
            RenderGraphResourceLifetime::TransientFrame
        ) || resource.external.is_some()
            || resource.sample_count == 0
        {
            return None;
        }

        let usage_class = ResourceUsageClass::from_resource_and_lifetime(resource, lifetime);
        let resource_class = usage_class.resource_class()?;
        match resource_class {
            ResourceClass::Texture => {
                let extent = resource.extent?;
                let format = resource.format?;
                Some(Self {
                    resource_class,
                    format: Some(format),
                    extent: Some(extent),
                    byte_size: None,
                    sample_count: resource.sample_count,
                    usage_class,
                })
            }
            ResourceClass::Buffer => {
                let byte_size = resource.byte_size.filter(|size| *size > 0)?;
                // MSAA is a texture allocation concept; a non-one sample count on
                // a buffer descriptor is treated as malformed/ineligible.
                if resource.sample_count != 1 {
                    return None;
                }
                Some(Self {
                    resource_class,
                    format: None,
                    extent: None,
                    byte_size: Some(byte_size),
                    sample_count: 1,
                    usage_class,
                })
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransientAllocationSlot {
    pub id: u32,
    pub compatibility: TransientResourceCompatibilityKey,
    pub resources: Vec<RenderGraphResourceId>,
    /// Envelope of all logical intervals assigned to the slot. There may be gaps
    /// inside this range; actual alias safety is checked against every interval.
    pub first_execution_index: u32,
    pub last_execution_index: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransientAliasGroup {
    pub id: u32,
    pub slot_id: u32,
    pub resources: Vec<RenderGraphResourceId>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransientResourceAllocationPlan {
    pub slots: Vec<TransientAllocationSlot>,
    #[serde(default)]
    pub alias_groups: Vec<TransientAliasGroup>,
    pub resource_to_slot: BTreeMap<RenderGraphResourceId, u32>,
    /// Live `TransientFrame` resources that did not expose enough compatible
    /// allocation metadata to participate safely in Phase 4 aliasing.
    #[serde(default)]
    pub ineligible_resources: Vec<RenderGraphResourceId>,
}

impl TransientResourceAllocationPlan {
    #[inline]
    pub fn slot_for(&self, resource: RenderGraphResourceId) -> Option<&TransientAllocationSlot> {
        let slot_id = self.resource_to_slot.get(&resource)?;
        self.slots.iter().find(|slot| slot.id == *slot_id)
    }

    /// Number of logical transient resources that can reuse an existing physical
    /// allocation instead of requiring another slot.
    #[inline]
    pub fn alias_reuse_count(&self) -> usize {
        self.slots
            .iter()
            .map(|slot| slot.resources.len().saturating_sub(1))
            .sum()
    }
}

/// Builds a deterministic first-fit transient allocation plan from Phase 3 live
/// lifetimes. This function never allocates GPU objects; providers consume the DTO
/// later and bind each slot to a backend-specific physical allocation.
pub fn plan_transient_resource_allocations(
    graph: &RenderGraphDesc,
    lifetimes: &RenderGraphResourceLifetimeReport,
) -> TransientResourceAllocationPlan {
    let descriptors = graph
        .resources
        .iter()
        .map(|resource| (resource.id, resource))
        .collect::<BTreeMap<_, _>>();
    let mut candidates = Vec::new();
    let mut ineligible_resources = Vec::new();
    for lifetime in &lifetimes.resources {
        let Some(resource) = descriptors.get(&lifetime.resource).copied() else {
            continue;
        };
        if !matches!(
            resource.lifetime,
            RenderGraphResourceLifetime::TransientFrame
        ) {
            continue;
        }
        if let Some(compatibility) =
            TransientResourceCompatibilityKey::from_resource(resource, lifetime)
        {
            candidates.push((resource.id, lifetime, compatibility));
        } else {
            ineligible_resources.push(resource.id);
        }
    }

    candidates.sort_by_key(|(resource, lifetime, _)| {
        (
            lifetime.first_execution_index,
            lifetime.last_execution_index,
            resource.0,
        )
    });

    let mut slots: Vec<TransientAllocationSlot> = Vec::new();
    let mut resource_to_slot = BTreeMap::new();

    for (resource, lifetime, compatibility) in candidates {
        // Candidates are processed in nondecreasing first-use order. Therefore the
        // latest end index assigned to a slot is sufficient for an inclusive overlap
        // test: if it is strictly before this resource starts, every earlier interval
        // in the slot is also disjoint. This keeps planning O(resources * slots) rather
        // than repeatedly scanning every logical resource already assigned to a slot.
        let reusable_slot = slots.iter().position(|slot| {
            slot.compatibility == compatibility
                && slot.last_execution_index < lifetime.first_execution_index
        });

        let slot_id = if let Some(slot_index) = reusable_slot {
            let slot = &mut slots[slot_index];
            slot.resources.push(resource);
            slot.first_execution_index = slot
                .first_execution_index
                .min(lifetime.first_execution_index);
            slot.last_execution_index =
                slot.last_execution_index.max(lifetime.last_execution_index);
            slot.id
        } else {
            let id = slots.len().min(u32::MAX as usize) as u32;
            slots.push(TransientAllocationSlot {
                id,
                compatibility,
                resources: vec![resource],
                first_execution_index: lifetime.first_execution_index,
                last_execution_index: lifetime.last_execution_index,
            });
            id
        };
        resource_to_slot.insert(resource, slot_id);
    }

    let alias_groups = slots
        .iter()
        .filter(|slot| slot.resources.len() > 1)
        .enumerate()
        .map(|(index, slot)| TransientAliasGroup {
            id: index.min(u32::MAX as usize) as u32,
            slot_id: slot.id,
            resources: slot.resources.clone(),
        })
        .collect();

    TransientResourceAllocationPlan {
        slots,
        alias_groups,
        resource_to_slot,
        ineligible_resources,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RenderGraphPassId, RenderGraphResourceUse, RenderGraphResourceUseKind};

    fn lifetime(resource: u64, first: u32, last: u32) -> ResourceLifetimeInterval {
        ResourceLifetimeInterval {
            resource: RenderGraphResourceId(resource),
            first_pass: RenderGraphPassId(first as u64 + 1),
            last_pass: RenderGraphPassId(last as u64 + 1),
            first_execution_index: first,
            last_execution_index: last,
            create_count: 0,
            read_count: 0,
            write_count: 1,
            history: vec![RenderGraphResourceUse::new(
                first,
                RenderGraphPassId(first as u64 + 1),
                RenderGraphResourceUseKind::Write,
                Some(RenderGraphResourceUsage::StorageTexture),
            )],
        }
    }

    fn texture(id: u64) -> RenderGraphResourceDesc {
        RenderGraphResourceDesc::transient_texture(
            RenderGraphResourceId(id),
            format!("r{id}"),
            RenderGraphResourceUsage::StorageTexture,
            Extent2D::new(64, 64),
            TextureFormat::Rgba16Float,
        )
    }

    #[test]
    fn non_overlapping_compatible_resources_share_one_slot() {
        let graph = RenderGraphDesc::new("alias")
            .add_resource(texture(1))
            .add_resource(texture(2));
        let lifetimes = RenderGraphResourceLifetimeReport {
            resources: vec![lifetime(1, 0, 1), lifetime(2, 2, 3)],
            unused_resources: Vec::new(),
        };

        let plan = plan_transient_resource_allocations(&graph, &lifetimes);
        assert_eq!(plan.slots.len(), 1);
        assert_eq!(
            plan.slots[0].resources,
            vec![RenderGraphResourceId(1), RenderGraphResourceId(2)]
        );
        assert_eq!(plan.alias_groups.len(), 1);
        assert_eq!(plan.alias_reuse_count(), 1);
        assert_eq!(plan.resource_to_slot[&RenderGraphResourceId(1)], 0);
        assert_eq!(plan.resource_to_slot[&RenderGraphResourceId(2)], 0);
    }

    #[test]
    fn touching_inclusive_lifetimes_do_not_alias() {
        let graph = RenderGraphDesc::new("overlap")
            .add_resource(texture(1))
            .add_resource(texture(2));
        let lifetimes = RenderGraphResourceLifetimeReport {
            resources: vec![lifetime(1, 0, 2), lifetime(2, 2, 4)],
            unused_resources: Vec::new(),
        };

        let plan = plan_transient_resource_allocations(&graph, &lifetimes);
        assert_eq!(plan.slots.len(), 2);
        assert!(plan.alias_groups.is_empty());
        assert_eq!(plan.alias_reuse_count(), 0);
    }

    #[test]
    fn sample_count_is_part_of_texture_compatibility() {
        let graph = RenderGraphDesc::new("msaa")
            .add_resource(texture(1))
            .add_resource(texture(2).with_sample_count(4));
        let lifetimes = RenderGraphResourceLifetimeReport {
            resources: vec![lifetime(1, 0, 1), lifetime(2, 2, 3)],
            unused_resources: Vec::new(),
        };

        let plan = plan_transient_resource_allocations(&graph, &lifetimes);
        assert_eq!(plan.slots.len(), 2);
        assert!(plan.alias_groups.is_empty());
    }

    #[test]
    fn usage_class_is_part_of_compatibility() {
        let graph = RenderGraphDesc::new("usage-class")
            .add_resource(texture(1))
            .add_resource(RenderGraphResourceDesc::transient_texture(
                RenderGraphResourceId(2),
                "r2",
                RenderGraphResourceUsage::ColorAttachment,
                Extent2D::new(64, 64),
                TextureFormat::Rgba16Float,
            ));
        let lifetimes = RenderGraphResourceLifetimeReport {
            resources: vec![lifetime(1, 0, 0), lifetime(2, 1, 1)],
            unused_resources: Vec::new(),
        };

        let plan = plan_transient_resource_allocations(&graph, &lifetimes);
        assert_eq!(plan.slots.len(), 2);
        assert!(plan.alias_groups.is_empty());
    }

    #[test]
    fn incomplete_live_transient_resource_is_reported_as_ineligible() {
        let mut incomplete = texture(1);
        incomplete.extent = None;
        let graph = RenderGraphDesc::new("ineligible").add_resource(incomplete);
        let lifetimes = RenderGraphResourceLifetimeReport {
            resources: vec![lifetime(1, 0, 0)],
            unused_resources: Vec::new(),
        };

        let plan = plan_transient_resource_allocations(&graph, &lifetimes);
        assert!(plan.slots.is_empty());
        assert!(plan.resource_to_slot.is_empty());
        assert_eq!(plan.ineligible_resources, vec![RenderGraphResourceId(1)]);
    }

    #[test]
    fn first_fit_reuses_slot_only_after_inclusive_lifetime_end() {
        let graph = RenderGraphDesc::new("first-fit")
            .add_resource(texture(1))
            .add_resource(texture(2))
            .add_resource(texture(3));
        let lifetimes = RenderGraphResourceLifetimeReport {
            resources: vec![lifetime(1, 0, 2), lifetime(2, 2, 3), lifetime(3, 3, 4)],
            unused_resources: Vec::new(),
        };

        let plan = plan_transient_resource_allocations(&graph, &lifetimes);
        assert_eq!(plan.slots.len(), 2);
        assert_eq!(
            plan.slots[0].resources,
            vec![RenderGraphResourceId(1), RenderGraphResourceId(3)]
        );
        assert_eq!(plan.slots[1].resources, vec![RenderGraphResourceId(2)]);
        assert_eq!(plan.resource_to_slot[&RenderGraphResourceId(1)], 0);
        assert_eq!(plan.resource_to_slot[&RenderGraphResourceId(2)], 1);
        assert_eq!(plan.resource_to_slot[&RenderGraphResourceId(3)], 0);
    }

    #[test]
    fn buffer_size_is_part_of_compatibility_and_supports_reuse() {
        let buffer_a = RenderGraphResourceDesc::transient_buffer(
            RenderGraphResourceId(1),
            "a",
            RenderGraphResourceUsage::StorageBuffer,
            4096,
        );
        let buffer_b = RenderGraphResourceDesc::transient_buffer(
            RenderGraphResourceId(2),
            "b",
            RenderGraphResourceUsage::StorageBuffer,
            4096,
        );
        let buffer_c = RenderGraphResourceDesc::transient_buffer(
            RenderGraphResourceId(3),
            "c",
            RenderGraphResourceUsage::StorageBuffer,
            8192,
        );
        let graph = RenderGraphDesc::new("buffers")
            .add_resource(buffer_a)
            .add_resource(buffer_b)
            .add_resource(buffer_c);
        let mut a = lifetime(1, 0, 0);
        a.history[0].usage = Some(RenderGraphResourceUsage::StorageBuffer);
        let mut b = lifetime(2, 1, 1);
        b.history[0].usage = Some(RenderGraphResourceUsage::StorageBuffer);
        let mut c = lifetime(3, 2, 2);
        c.history[0].usage = Some(RenderGraphResourceUsage::StorageBuffer);
        let lifetimes = RenderGraphResourceLifetimeReport {
            resources: vec![a, b, c],
            unused_resources: Vec::new(),
        };

        let plan = plan_transient_resource_allocations(&graph, &lifetimes);
        assert_eq!(plan.slots.len(), 2);
        assert_eq!(plan.alias_groups.len(), 1);
        assert_eq!(plan.alias_reuse_count(), 1);
    }
}
