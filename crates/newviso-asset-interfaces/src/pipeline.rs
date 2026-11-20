//! Asset/world data pipeline DTOs.
//!
//! `engine.assets` owns bytes, VFS, package/source mounts, import queue,
//! dirty scan, dependency graph, UID registry, thumbnails and package writer
//! capability visibility. Semantic interpretation stays in domain gateways
//! (`engine.assets.models`, `engine.assets.materials`, `engine.assets.textures`,
//! `engine.assets.definitions`, `engine.scene`, ...). Renderers consume only
//! render-ready packets produced after those domain gateways validate data.

use serde::{Deserialize, Serialize};

pub const ASSET_PIPELINE_STATUS_SCHEMA: &str = "northstar.assets.pipeline.status.v1";
pub const ASSET_FORMAT_OWNERSHIP_SCHEMA: &str = "northstar.assets.format.ownership.v1";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AssetFormatOwnership {
    pub extension: String,
    pub role: String,
    pub byte_owner_gateway: String,
    pub semantic_owner_gateway: String,
    pub runtime_rule: String,
}

impl Default for AssetFormatOwnership {
    fn default() -> Self {
        Self {
            extension: String::new(),
            role: String::new(),
            byte_owner_gateway: crate::ENGINE_ASSET_SERVICE_ID.to_owned(),
            semantic_owner_gateway: String::new(),
            runtime_rule: String::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AssetPipelineStatusV1 {
    pub schema: String,
    pub vfs_online: bool,
    pub packages_online: bool,
    pub source_mounts_online: bool,
    pub import_queue_online: bool,
    pub dirty_scan_online: bool,
    pub dependency_graph_online: bool,
    pub uid_registry_online: bool,
    pub thumbnails_online: bool,
    pub package_writer_online: bool,
    pub warnings: Vec<String>,
}

impl Default for AssetPipelineStatusV1 {
    fn default() -> Self {
        Self {
            schema: ASSET_PIPELINE_STATUS_SCHEMA.to_owned(),
            vfs_online: false,
            packages_online: false,
            source_mounts_online: false,
            import_queue_online: false,
            dirty_scan_online: false,
            dependency_graph_online: false,
            uid_registry_online: false,
            thumbnails_online: false,
            package_writer_online: false,
            warnings: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AssetPipelineSnapshotV1 {
    pub schema: String,
    pub ownership: Vec<AssetFormatOwnership>,
    pub status: AssetPipelineStatusV1,
}

impl Default for AssetPipelineSnapshotV1 {
    fn default() -> Self {
        Self {
            schema: ASSET_PIPELINE_STATUS_SCHEMA.to_owned(),
            ownership: Vec::new(),
            status: AssetPipelineStatusV1::default(),
        }
    }
}

/// `ownership` is populated at runtime from `engine.assets.types`; the API does not
/// carry a compiled first-party format catalog.
pub const ASSET_IMPORTER_DESCRIPTOR_SCHEMA: &str = "northstar.assets.importer_descriptor.v1";
pub const ASSET_RUNTIME_GRAPH_SCHEMA: &str = "northstar.assets.runtime_graph.v1";
pub const ASSET_INVALIDATION_PLAN_SCHEMA: &str = "northstar.assets.invalidation_plan.v1";
pub const ASSET_CACHE_KEY_SCHEMA: &str = "northstar.assets.cache_key.v1";
pub const NEPAK_PACKAGE_WRITER_CAPABILITY_ID: &str = "assets.package_writer.nepak";
pub const ASSET_PACKAGE_WRITE_NEPAK_JSON_V1: &str = "asset.package_write_nepak_json_v1";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ImporterDescriptorV1 {
    pub importer_id: String,
    pub label: String,
    pub source_extensions: Vec<String>,
    pub source_content_kinds: Vec<String>,
    pub runtime_outputs: Vec<String>,
    pub owner_gateway: String,
    pub required_capability: Option<String>,
    pub cache_key_inputs: Vec<String>,
    pub deterministic: bool,
}

impl Default for ImporterDescriptorV1 {
    fn default() -> Self {
        Self {
            importer_id: String::new(),
            label: String::new(),
            source_extensions: Vec::new(),
            source_content_kinds: Vec::new(),
            runtime_outputs: Vec::new(),
            owner_gateway: crate::ENGINE_ASSET_SERVICE_ID.to_owned(),
            required_capability: None,
            cache_key_inputs: vec![
                "source_path".to_owned(),
                "source_content_hash".to_owned(),
                "importer_id".to_owned(),
                "importer_version".to_owned(),
                "settings_hash".to_owned(),
            ],
            deterministic: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AssetCacheKeyV1 {
    pub schema: String,
    pub source_ref: String,
    pub source_hash: String,
    pub importer_id: String,
    pub importer_version: String,
    pub settings_hash: String,
    pub platform: String,
    pub cache_key: String,
}

impl Default for AssetCacheKeyV1 {
    fn default() -> Self {
        Self {
            schema: ASSET_CACHE_KEY_SCHEMA.to_owned(),
            source_ref: String::new(),
            source_hash: String::new(),
            importer_id: String::new(),
            importer_version: String::new(),
            settings_hash: String::new(),
            platform: "any".to_owned(),
            cache_key: String::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SourceAssetNodeV1 {
    pub source_ref: String,
    pub content_hash: String,
    pub content_kind: String,
    pub importer_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct RuntimeAssetNodeV1 {
    pub asset_ref: String,
    pub content_kind: String,
    pub owner_gateway: String,
    pub cache_key: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AssetDependencyEdgeV1 {
    pub from_ref: String,
    pub to_ref: String,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AssetRuntimeGraphV1 {
    pub schema: String,
    pub sources: Vec<SourceAssetNodeV1>,
    pub runtime_assets: Vec<RuntimeAssetNodeV1>,
    pub dependencies: Vec<AssetDependencyEdgeV1>,
}

impl Default for AssetRuntimeGraphV1 {
    fn default() -> Self {
        Self {
            schema: ASSET_RUNTIME_GRAPH_SCHEMA.to_owned(),
            sources: Vec::new(),
            runtime_assets: Vec::new(),
            dependencies: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AssetInvalidationPlanV1 {
    pub schema: String,
    pub changed_sources: Vec<String>,
    /// Full transitive affected ref set, including changed roots and reverse dependents.
    pub affected_refs: Vec<String>,
    /// Reverse-dependency invalidation order, nearest changed roots first.
    pub invalidation_order: Vec<String>,
    pub invalidated_cache_keys: Vec<String>,
    pub affected_runtime_assets: Vec<String>,
    pub cycles: Vec<Vec<String>>,
    pub reason: String,
}

impl Default for AssetInvalidationPlanV1 {
    fn default() -> Self {
        Self {
            schema: ASSET_INVALIDATION_PLAN_SCHEMA.to_owned(),
            changed_sources: Vec::new(),
            affected_refs: Vec::new(),
            invalidation_order: Vec::new(),
            invalidated_cache_keys: Vec::new(),
            affected_runtime_assets: Vec::new(),
            cycles: Vec::new(),
            reason: String::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AssetInvalidationRequestV1 {
    pub changed_sources: Vec<String>,
    pub reason: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AssetDependencyIndexV1 {
    /// `dependency -> direct dependents`.
    pub reverse_dependents: std::collections::BTreeMap<String, Vec<String>>,
    /// `dependent -> direct dependencies`.
    pub dependencies: std::collections::BTreeMap<String, Vec<String>>,
}

impl AssetDependencyIndexV1 {
    pub fn from_graph(graph: &AssetRuntimeGraphV1) -> Self {
        let mut reverse =
            std::collections::BTreeMap::<String, std::collections::BTreeSet<String>>::new();
        let mut forward =
            std::collections::BTreeMap::<String, std::collections::BTreeSet<String>>::new();
        for edge in &graph.dependencies {
            let from = normalize_graph_ref(&edge.from_ref);
            let to = normalize_graph_ref(&edge.to_ref);
            if from.is_empty() || to.is_empty() || from == to {
                continue;
            }
            forward.entry(from.clone()).or_default().insert(to.clone());
            reverse.entry(to).or_default().insert(from);
        }
        Self {
            reverse_dependents: reverse
                .into_iter()
                .map(|(key, values)| (key, values.into_iter().collect()))
                .collect(),
            dependencies: forward
                .into_iter()
                .map(|(key, values)| (key, values.into_iter().collect()))
                .collect(),
        }
    }

    pub fn transitive_dependents(&self, changed: &[String]) -> Vec<String> {
        let mut visited = std::collections::BTreeSet::<String>::new();
        let mut queue = std::collections::VecDeque::<String>::new();
        for item in changed {
            let item = normalize_graph_ref(item);
            if !item.is_empty() && visited.insert(item.clone()) {
                queue.push_back(item);
            }
        }
        while let Some(current) = queue.pop_front() {
            if let Some(dependents) = self.reverse_dependents.get(&current) {
                for dependent in dependents {
                    if visited.insert(dependent.clone()) {
                        queue.push_back(dependent.clone());
                    }
                }
            }
        }
        visited.into_iter().collect()
    }

    /// Deterministic breadth-first invalidation order. Roots are first, then direct and
    /// transitive dependents; lexical ordering breaks ties so hot-reload is reproducible.
    pub fn invalidation_order(&self, changed: &[String]) -> Vec<String> {
        let mut seen = std::collections::BTreeSet::<String>::new();
        let mut current = changed
            .iter()
            .map(|value| normalize_graph_ref(value))
            .filter(|value| !value.is_empty())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let mut out = Vec::new();
        while !current.is_empty() {
            let mut next = std::collections::BTreeSet::<String>::new();
            for item in current {
                if !seen.insert(item.clone()) {
                    continue;
                }
                out.push(item.clone());
                if let Some(dependents) = self.reverse_dependents.get(&item) {
                    for dependent in dependents {
                        if !seen.contains(dependent) {
                            next.insert(dependent.clone());
                        }
                    }
                }
            }
            current = next.into_iter().collect();
        }
        out
    }
}

pub fn plan_asset_invalidation_v1(
    graph: &AssetRuntimeGraphV1,
    request: AssetInvalidationRequestV1,
) -> AssetInvalidationPlanV1 {
    let changed_sources = request
        .changed_sources
        .into_iter()
        .map(|value| normalize_graph_ref(&value))
        .filter(|value| !value.is_empty())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let index = AssetDependencyIndexV1::from_graph(graph);
    let invalidation_order = index.invalidation_order(&changed_sources);
    let affected_set = invalidation_order
        .iter()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();

    let mut invalidated_cache_keys = std::collections::BTreeSet::<String>::new();
    let mut affected_runtime_assets = std::collections::BTreeSet::<String>::new();
    for asset in &graph.runtime_assets {
        let asset_ref = normalize_graph_ref(&asset.asset_ref);
        if affected_set.contains(&asset_ref) {
            affected_runtime_assets.insert(asset_ref);
            let cache_key = asset.cache_key.trim();
            if !cache_key.is_empty() {
                invalidated_cache_keys.insert(cache_key.to_owned());
            }
        }
    }

    AssetInvalidationPlanV1 {
        schema: ASSET_INVALIDATION_PLAN_SCHEMA.to_owned(),
        changed_sources,
        affected_refs: affected_set.into_iter().collect(),
        invalidation_order,
        invalidated_cache_keys: invalidated_cache_keys.into_iter().collect(),
        affected_runtime_assets: affected_runtime_assets.into_iter().collect(),
        cycles: detect_dependency_cycles(&index),
        reason: request.reason,
    }
}

fn normalize_graph_ref(value: &str) -> String {
    value.trim().replace('\\', "/")
}

fn detect_dependency_cycles(index: &AssetDependencyIndexV1) -> Vec<Vec<String>> {
    fn visit(
        node: &str,
        index: &AssetDependencyIndexV1,
        visiting: &mut Vec<String>,
        visited: &mut std::collections::BTreeSet<String>,
        cycles: &mut std::collections::BTreeSet<Vec<String>>,
    ) {
        if let Some(position) = visiting.iter().position(|item| item == node) {
            let mut cycle = visiting[position..].to_vec();
            cycle.push(node.to_owned());
            // Normalize cycle rotation for deterministic de-duplication.
            if cycle.len() > 2 {
                let body = &cycle[..cycle.len() - 1];
                if let Some((min_index, _)) =
                    body.iter().enumerate().min_by_key(|(_, value)| *value)
                {
                    let mut normalized = body[min_index..].to_vec();
                    normalized.extend_from_slice(&body[..min_index]);
                    normalized.push(normalized[0].clone());
                    cycles.insert(normalized);
                }
            }
            return;
        }
        if !visited.insert(node.to_owned()) {
            return;
        }
        visiting.push(node.to_owned());
        if let Some(deps) = index.dependencies.get(node) {
            for dependency in deps {
                visit(dependency, index, visiting, visited, cycles);
            }
        }
        visiting.pop();
    }

    let mut visited = std::collections::BTreeSet::new();
    let mut cycles = std::collections::BTreeSet::new();
    for node in index.dependencies.keys() {
        visit(node, index, &mut Vec::new(), &mut visited, &mut cycles);
    }
    cycles.into_iter().collect()
}

#[cfg(test)]
mod invalidation_tests {
    use super::*;

    #[test]
    fn reverse_dependency_plan_is_transitive_and_deterministic() {
        // model -> material -> texture; changing texture invalidates all three.
        let graph = AssetRuntimeGraphV1 {
            runtime_assets: vec![
                RuntimeAssetNodeV1 {
                    asset_ref: "game:/model.ydd".into(),
                    cache_key: "model-cache".into(),
                    ..Default::default()
                },
                RuntimeAssetNodeV1 {
                    asset_ref: "game:/material.nemat".into(),
                    cache_key: "material-cache".into(),
                    ..Default::default()
                },
                RuntimeAssetNodeV1 {
                    asset_ref: "game:/texture.ytd".into(),
                    cache_key: "texture-cache".into(),
                    ..Default::default()
                },
            ],
            dependencies: vec![
                AssetDependencyEdgeV1 {
                    from_ref: "game:/model.ydd".into(),
                    to_ref: "game:/material.nemat".into(),
                    reason: "material".into(),
                },
                AssetDependencyEdgeV1 {
                    from_ref: "game:/material.nemat".into(),
                    to_ref: "game:/texture.ytd".into(),
                    reason: "texture".into(),
                },
            ],
            ..Default::default()
        };
        let plan = plan_asset_invalidation_v1(
            &graph,
            AssetInvalidationRequestV1 {
                changed_sources: vec!["game:/texture.ytd".into()],
                reason: "file watcher".into(),
            },
        );
        assert_eq!(
            plan.invalidation_order,
            vec![
                "game:/texture.ytd",
                "game:/material.nemat",
                "game:/model.ydd",
            ]
        );
        assert_eq!(
            plan.invalidated_cache_keys,
            vec!["material-cache", "model-cache", "texture-cache",]
        );
    }

    #[test]
    fn dependency_cycles_are_reported_without_infinite_walk() {
        let graph = AssetRuntimeGraphV1 {
            dependencies: vec![
                AssetDependencyEdgeV1 {
                    from_ref: "a".into(),
                    to_ref: "b".into(),
                    reason: String::new(),
                },
                AssetDependencyEdgeV1 {
                    from_ref: "b".into(),
                    to_ref: "a".into(),
                    reason: String::new(),
                },
            ],
            ..Default::default()
        };
        let plan = plan_asset_invalidation_v1(
            &graph,
            AssetInvalidationRequestV1 {
                changed_sources: vec!["a".into()],
                reason: String::new(),
            },
        );
        assert_eq!(plan.affected_refs, vec!["a", "b"]);
        assert_eq!(plan.cycles.len(), 1);
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct NepakPackageWriteEntryV1 {
    pub target_path: String,
    pub source_ref: String,
    pub payload_base64: String,
    pub content_kind: String,
    pub cache_key: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct NepakPackageWriteRequestV1 {
    pub package_ref: String,
    pub runtime_graph: AssetRuntimeGraphV1,
    pub entries: Vec<String>,
    pub entry_payloads: Vec<NepakPackageWriteEntryV1>,
    pub deterministic_order: bool,
    pub dry_run: bool,
    pub requested_capability: String,
}

impl Default for NepakPackageWriteRequestV1 {
    fn default() -> Self {
        Self {
            package_ref: String::new(),
            runtime_graph: AssetRuntimeGraphV1::default(),
            entries: Vec::new(),
            entry_payloads: Vec::new(),
            deterministic_order: true,
            dry_run: false,
            requested_capability: NEPAK_PACKAGE_WRITER_CAPABILITY_ID.to_owned(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct NepakPackageWriteResponseV1 {
    pub ok: bool,
    pub package_ref: String,
    pub written_entries: Vec<String>,
    pub skipped_entries: Vec<String>,
    pub package_hash: String,
    pub diagnostics: Vec<String>,
}

/// Explicit UTF-8 text replacement through the AssetManager package-writer
/// capability. This is a VFS write contract, not a direct filesystem path.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextAssetWriteRequestV1 {
    pub logical_path: String,
    pub text: String,
    pub expected_hash: String,
    pub requested_capability: String,
}

impl Default for TextAssetWriteRequestV1 {
    fn default() -> Self {
        Self {
            logical_path: String::new(),
            text: String::new(),
            expected_hash: String::new(),
            requested_capability: crate::ASSETS_PACKAGE_WRITER_CAPABILITY_ID.to_owned(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct TextAssetWriteResponseV1 {
    pub ok: bool,
    pub written: bool,
    pub logical_path: String,
    pub bytes_written: u64,
    pub content_hash: String,
    pub diagnostics: Vec<String>,
}
