#![forbid(unsafe_op_in_unsafe_fn)]

use abi_stable::std_types::{ROption, RString, RVec};
use abi_stable::StableAbi;

use crate::types::CapabilityId;

pub use newengine_service_api::{BackendRouteDescriptor, BackendServiceSpec};

pub const CAPABILITY_TAG_RETIRED: &str = "retired";
pub const CAPABILITY_TAG_RENDER: &str = "render";
pub const CAPABILITY_TAG_RUNTIME: &str = "runtime";
/// Marks capabilities that enable live authoring/editing tooling.
pub const CAPABILITY_TAG_EDITING: &str = "editing";
/// Optional capability that makes editor/live-authoring tools available over the current runtime world.
pub const CAPABILITY_ID_EDITING_TOOLS: &str = "engine.editing.tools";
pub const CAPABILITY_ID_RENDER_DRAW_LIST_PROVIDER: &str = "render.draw_list_provider";
pub const CAPABILITY_ID_RENDER_LIGHT_EXTRACTION_PROVIDER: &str = "render.light_extraction_provider";
/// Optional importer that turns opaque SpeedTree .srt sources into engine runtime assets.
pub const CAPABILITY_ID_FOLIAGE_SRT_IMPORTER: &str = "assets.models.foliage.srt_importer";
/// Optional importer that turns opaque SpeedTree Modeler .spm sources into engine runtime assets.
pub const CAPABILITY_ID_FOLIAGE_SPM_IMPORTER: &str = "assets.models.foliage.spm_importer";
/// Optional GPU culling/indirect adapter. Generic CPU foliage extraction is mandatory.
pub const CAPABILITY_ID_RENDER_FOLIAGE_GPU_CULLING: &str = "render.foliage.gpu_culling";

#[inline]
pub fn capability_json_has_tag(describe_json: &str, tag: &str) -> bool {
    let tag = tag.trim();
    if tag.is_empty() {
        return false;
    }

    let compact = describe_json
        .chars()
        .filter(|ch| !ch.is_ascii_whitespace())
        .collect::<String>();
    if !compact.contains("\"tags\":[") {
        return false;
    }

    let quoted = format!("\"{}\"", tag);
    compact
        .split("\"tags\":[")
        .skip(1)
        .filter_map(|tail| tail.split(']').next())
        .any(|array| array.split(',').any(|entry| entry == quoted))
}

#[inline]
pub fn capability_has_tag(capability: &CapabilityDesc, tag: &str) -> bool {
    CapabilityDescV2::from_legacy(capability).has_tag(tag)
}

include!("capability/v2_capability.rs");
include!("capability/v2_plugin.rs");
include!("capability/metadata.rs");
include!("capability/v1_compat.rs");

#[cfg(test)]
#[path = "capability/tests.rs"]
mod typed_metadata_tests;
