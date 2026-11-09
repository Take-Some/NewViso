use newviso_config::parse_engine_config_xml;
use newviso_project::ResolvedProject;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

pub struct DirectoryAssets {
    mounts: Vec<(i64, usize, String, PathBuf)>,
    pub bindings: BTreeMap<String, PathBuf>,
}

impl DirectoryAssets {
    pub fn new(project: &ResolvedProject, shared: &Path) -> Result<Self, String> {
        let path = shared.join("config/engine/vfs_mounts.xml");
        let policy = parse_engine_config_xml(
            &fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?,
            &path,
        )?;
        if policy["schema"] != "newviso.runtime.vfs_mount_policy.v1" {
            return Err("unsupported VFS mount policy".into());
        }
        let mut mounts = Vec::new();
        for (i, (key, root)) in [
            ("shared_assets", shared),
            ("project_root", project.root.as_path()),
            ("project_assets", project.assets_dir.as_path()),
        ]
        .into_iter()
        .enumerate()
        {
            let priority = policy[key]["priority"]
                .as_i64()
                .ok_or_else(|| format!("invalid {key} priority"))?;
            let prefix = policy[key]["mount"]
                .as_str()
                .ok_or_else(|| format!("invalid {key} mount"))?
                .trim_matches('/')
                .to_owned();
            mounts.push((priority, i, prefix, root.to_path_buf()));
        }
        mounts.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
        Ok(Self {
            mounts,
            bindings: BTreeMap::new(),
        })
    }

    pub fn json(&mut self, logical: &str) -> Result<Value, String> {
        let logical = logical.replace('\\', "/");
        if logical.is_empty()
            || logical.starts_with('/')
            || logical.contains(':')
            || logical.split('/').any(|v| v == "..")
        {
            return Err("logical asset path escapes mounts".into());
        }
        for (_, _, prefix, root) in &self.mounts {
            let relative = if prefix.is_empty() {
                logical.as_str()
            } else if let Some(path) = logical.strip_prefix(&format!("{prefix}/")) {
                path
            } else {
                continue;
            };
            let file = root.join(relative);
            if !file.is_file() {
                continue;
            }
            let canonical = file.canonicalize().map_err(|e| e.to_string())?;
            let canonical_root = root.canonicalize().map_err(|e| e.to_string())?;
            if !canonical.starts_with(canonical_root) {
                return Err("asset link escapes mount root".into());
            }
            let metadata = fs::metadata(&file).map_err(|e| e.to_string())?;
            if metadata.len() > 64 * 1024 * 1024 {
                return Err(format!(
                    "configuration file exceeds 64 MiB: {}",
                    file.display()
                ));
            }
            let bytes = fs::read(&file).map_err(|e| format!("{}: {e}", file.display()))?;
            let value =
                serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", file.display()))?;
            self.bindings.insert(logical, file);
            return Ok(value);
        }
        Err(format!(
            "configuration asset '{logical}' is missing from configured filesystem mounts"
        ))
    }
}
