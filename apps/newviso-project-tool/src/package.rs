use newviso_config::ResolvedBootstrapConfig;
use newviso_project::{ResolvedProject, ResolvedRuntimeConfiguration};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};

#[derive(Debug)]
pub struct PackagePlan {
    files: BTreeMap<String, PathBuf>,
    generated: BTreeMap<String, Vec<u8>>,
    total_bytes: u64,
    roots: Vec<PathBuf>,
    runtime_settings: String,
}

fn safe_relative(path: &str) -> Result<&Path, String> {
    let p = Path::new(path);
    if path.is_empty()
        || path.contains('\\')
        || path.contains(':')
        || p.is_absolute()
        || p.components().any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(format!("invalid package relative path '{path}'"));
    }
    Ok(p)
}

fn inventory(
    root: &Path,
    current: &Path,
    prefix: &str,
    files: &mut BTreeMap<String, PathBuf>,
) -> Result<(), String> {
    let meta = fs::symlink_metadata(current).map_err(|e| format!("{}: {e}", current.display()))?;
    if meta.file_type().is_symlink() || meta.file_attributes_reparse() {
        return Err(format!(
            "package inputs cannot contain links: {}",
            current.display()
        ));
    }
    if meta.is_dir() {
        for entry in fs::read_dir(current).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            if [".newviso", ".git", "target", "tools"]
                .iter()
                .any(|n| entry.file_name() == *n)
            {
                continue;
            }
            inventory(root, &entry.path(), prefix, files)?;
        }
    } else if meta.is_file() {
        let rel = current
            .strip_prefix(root)
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        let dest = format!("{prefix}/{rel}");
        safe_relative(&dest)?;
        if files.insert(dest.clone(), current.to_owned()).is_some() {
            return Err(format!("duplicate package destination {dest}"));
        }
    }
    Ok(())
}

trait Reparse {
    fn file_attributes_reparse(&self) -> bool;
}
impl Reparse for fs::Metadata {
    fn file_attributes_reparse(&self) -> bool {
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            self.file_attributes() & 0x400 != 0
        }
        #[cfg(not(windows))]
        {
            false
        }
    }
}

impl PackagePlan {
    pub fn new(
        project: &ResolvedProject,
        bootstrap: &ResolvedBootstrapConfig,
        resolved: &ResolvedRuntimeConfiguration,
        engine: &Path,
        runtime_source: &Path,
    ) -> Result<Self, String> {
        let engine = engine
            .canonicalize()
            .map_err(|e| format!("engine executable: {e}"))?;
        let mut files = BTreeMap::new();
        inventory(&project.root, &project.root, "game", &mut files)?;
        inventory(
            &bootstrap.assets_dir,
            &bootstrap.assets_dir,
            "shared",
            &mut files,
        )?;
        inventory(
            &bootstrap.provider_dir,
            &bootstrap.provider_dir,
            "providers",
            &mut files,
        )?;
        // A separately configured codec deployment remains separate from provider discovery.
        let codecs = bootstrap
            .codecs_dir
            .canonicalize()
            .map_err(|e| format!("codec directory: {e}"))?;
        let provider_codecs = bootstrap.provider_dir.join("codecs").canonicalize().ok();
        if provider_codecs.as_ref() != Some(&codecs) {
            inventory(&codecs, &codecs, "codecs", &mut files)?;
        }
        let codec_path = if provider_codecs.as_ref() == Some(&codecs) {
            "providers/codecs"
        } else {
            "codecs"
        };
        files.insert("newviso.exe".into(), engine);
        let runtime_source = runtime_source.canonicalize().map_err(|e| e.to_string())?;
        let runtime_settings = files
            .iter()
            .find_map(|(dest, source)| {
                (source.canonicalize().ok().as_ref() == Some(&runtime_source)).then(|| dest.clone())
            })
            .ok_or("runtime settings are outside packaged mounts")?;
        let mut manifest: Value =
            serde_json::from_slice(&fs::read(&project.manifest_path).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        manifest["configuration"] = json!({});
        manifest["paths"]["cache"] = json!(".newviso/cache");
        let generated=BTreeMap::from([
            ("game/project.json".into(),serde_json::to_vec_pretty(&manifest).map_err(|e|e.to_string())?),
            (runtime_settings.clone(),serde_json::to_vec_pretty(&resolved.settings).map_err(|e|e.to_string())?),
            ("config.json".into(),serde_json::to_vec_pretty(&json!({"paths":{"base":".","providers":"providers","assets":"shared","content":"shared","cache":"cache","codecs":codec_path},
                "providers":{"logging":bootstrap.logging_provider,"input":bootstrap.input_provider,"assets":bootstrap.assets_provider,"ecs":bootstrap.ecs_provider,"platform":bootstrap.platform_provider,"renderer":bootstrap.renderer_provider}})).map_err(|e|e.to_string())?),
            ("Launch.cmd".into(),format!("@echo off\r\nsetlocal EnableExtensions\r\ncd /d \"%~dp0\"\r\nset \"ROOT-DIR=%~dp0\"\r\nset \"PROJECT-DIR=%~dp0game\"\r\nset \"NEWENGINE_SHARED_CONTENT_ROOT=%~dp0shared\"\r\nset \"VULKAN_SDK=%~dp0providers\\toolchains\\vulkan-sdk\"\r\nset \"NEWENGINE_GLSLANG_VALIDATOR=%~dp0providers\\toolchains\\vulkan-sdk\\Bin\\glslangValidator.exe\"\r\n\"%~dp0newviso.exe\" --config \"%~dp0config.json\" --base-dir \"%~dp0.\" --project \"%~dp0game\" --provider-dir \"%~dp0providers\" --assets-dir \"%~dp0shared\" --content-dir \"%~dp0shared\" --codecs-dir \"%~dp0{}\" %*\r\nexit /b %errorlevel%\r\n",codec_path.replace('/',"\\")).into_bytes()),
        ]);
        for path in generated.keys() {
            files.remove(path);
        }
        let total_bytes = files
            .values()
            .try_fold(0u64, |sum, p| fs::metadata(p).map(|m| sum + m.len()))
            .map_err(|e| e.to_string())?
            + generated.values().map(|b| b.len() as u64).sum::<u64>();
        Ok(Self {
            files,
            generated,
            total_bytes,
            roots: vec![
                project.root.clone(),
                bootstrap.assets_dir.clone(),
                bootstrap.provider_dir.clone(),
                codecs,
            ],
            runtime_settings,
        })
    }
    pub fn summary(&self) -> Value {
        json!({"schema":"newviso.package-plan.v1","layout":"loose_directory","closure":"full_shared_and_project_mounts","files":self.files.len()+self.generated.len(),"bytes":self.total_bytes,
        "note":"Includes all mounted shared content and deployed providers; no source-format guessing or dependency pruning."})
    }
    pub fn build(&self, output: &Path) -> Result<Value, String> {
        if output.exists() {
            return Err("package output already exists; choose a new directory".into());
        }
        let parent = output
            .parent()
            .ok_or("package output needs a parent directory")?;
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let parent = parent.canonicalize().map_err(|e| e.to_string())?;
        let output = parent.join(output.file_name().ok_or("package directory name missing")?);
        for root in &self.roots {
            let canonical = root.canonicalize().map_err(|e| e.to_string())?;
            if output.starts_with(&canonical) {
                return Err("package destination must be outside all input roots".into());
            }
        }
        for source in self.files.values() {
            let source = source.canonicalize().map_err(|e| e.to_string())?;
            if output.starts_with(&source) {
                return Err("package destination overlaps an input file".into());
            }
        }
        let stage = parent.join(format!(
            ".newviso-package-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_nanos()
        ));
        fs::create_dir(&stage).map_err(|e| e.to_string())?;
        let result = (|| {
            let mut entries = Vec::new();
            for (relative, source) in &self.files {
                let target = stage.join(safe_relative(relative)?);
                fs::create_dir_all(target.parent().unwrap()).map_err(|e| e.to_string())?;
                let mut input = fs::File::open(source).map_err(|e| e.to_string())?;
                let mut dest = fs::File::create(&target).map_err(|e| e.to_string())?;
                let (bytes, hash) = transfer_hash(&mut input, Some(&mut dest))?;
                dest.sync_all().map_err(|e| e.to_string())?;
                entries.push(json!({"path":relative,"bytes":bytes,"blake3":hash}));
            }
            for (relative, bytes) in &self.generated {
                let target = stage.join(safe_relative(relative)?);
                fs::create_dir_all(target.parent().unwrap()).map_err(|e| e.to_string())?;
                fs::write(target, bytes).map_err(|e| e.to_string())?;
                entries.push(json!({"path":relative,"bytes":bytes.len(),"blake3":blake3::hash(bytes).to_hex().to_string()}));
            }
            entries.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
            let manifest = json!({"schema":"newviso.package.v1","hash":"blake3","layout":"loose_directory","runtime_settings":self.runtime_settings,"entries":entries});
            fs::write(
                stage.join("package.manifest.json"),
                serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
            let integrity = verify(&stage)?;
            fs::rename(&stage, &output).map_err(|e| e.to_string())?;
            Ok(
                json!({"passed":true,"output":output,"plan":self.summary(),"integrity":integrity,"manifest_path":output.join("package.manifest.json")}),
            )
        })();
        if result.is_err() {
            let _ = fs::remove_dir_all(stage);
        }
        result
    }
}

fn transfer_hash(
    input: &mut fs::File,
    mut output: Option<&mut fs::File>,
) -> Result<(u64, String), String> {
    let mut hasher = blake3::Hasher::new();
    let mut bytes = 0;
    let mut buf = vec![0; 1024 * 1024];
    loop {
        let count = input.read(&mut buf).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        hasher.update(&buf[..count]);
        bytes += count as u64;
        if let Some(out) = &mut output {
            out.write_all(&buf[..count]).map_err(|e| e.to_string())?;
        }
    }
    Ok((bytes, hasher.finalize().to_hex().to_string()))
}

pub fn verify(root: &Path) -> Result<Value, String> {
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let manifest: Value = serde_json::from_slice(
        &fs::read(root.join("package.manifest.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    if manifest["schema"] != "newviso.package.v1" || manifest["hash"] != "blake3" {
        return Err("unsupported package manifest".into());
    }
    let entries = manifest["entries"]
        .as_array()
        .ok_or("package entries missing")?;
    let mut seen = std::collections::BTreeSet::new();
    let mut bytes = 0;
    for entry in entries {
        let relative = entry["path"].as_str().ok_or("package path missing")?;
        safe_relative(relative)?;
        if !seen.insert(relative) {
            return Err("duplicate package entry".into());
        }
        let path = root.join(relative);
        let canonical = path
            .canonicalize()
            .map_err(|e| format!("{relative}: {e}"))?;
        if !canonical.starts_with(&root) {
            return Err("package link escapes root".into());
        }
        let (size, hash) =
            transfer_hash(&mut fs::File::open(path).map_err(|e| e.to_string())?, None)?;
        if entry["bytes"].as_u64() != Some(size) || entry["blake3"].as_str() != Some(&hash) {
            return Err(format!("package integrity mismatch: {relative}"));
        }
        bytes += size;
    }
    for required in [
        "newviso.exe",
        "Launch.cmd",
        "config.json",
        "game/project.json",
    ] {
        if !seen.contains(required) {
            return Err(format!("package required file missing: {required}"));
        }
    }
    let runtime = manifest["runtime_settings"]
        .as_str()
        .ok_or("packaged runtime settings missing")?;
    if !seen.contains(runtime) {
        return Err("packaged runtime settings entry missing".into());
    }
    let mut actual = std::collections::BTreeSet::new();
    verify_inventory(&root, &root, &mut actual)?;
    actual.remove("package.manifest.json");
    if actual
        .iter()
        .map(String::as_str)
        .collect::<std::collections::BTreeSet<_>>()
        != seen
    {
        return Err("package contains untracked files".into());
    }
    Ok(json!({"passed":true,"files":entries.len(),"bytes":bytes}))
}

fn verify_inventory(
    root: &Path,
    current: &Path,
    out: &mut std::collections::BTreeSet<String>,
) -> Result<(), String> {
    for entry in fs::read_dir(current).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        let relative = path
            .strip_prefix(root)
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        if relative == "game/.newviso" || relative == "cache" {
            continue;
        }
        let meta = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
        if meta.file_type().is_symlink() || meta.file_attributes_reparse() {
            return Err("package contains a link".into());
        }
        if meta.is_dir() {
            verify_inventory(root, &path, out)?;
        } else if meta.is_file() {
            out.insert(relative);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn packages_detect_tampering_extra_files_and_refuse_overwrite() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("newviso-package-{nonce}"));
        fs::create_dir_all(root.join("input")).unwrap();
        let source = root.join("input/source");
        fs::write(&source, b"original").unwrap();
        let plan = PackagePlan {
            files: BTreeMap::from([("newviso.exe".into(), source)]),
            generated: [
                "Launch.cmd",
                "config.json",
                "game/project.json",
                "game/runtime.json",
            ]
            .map(|p| (p.into(), b"fixture".to_vec()))
            .into_iter()
            .collect(),
            total_bytes: 36,
            roots: vec![root.join("input")],
            runtime_settings: "game/runtime.json".into(),
        };
        let output = root.join("output");
        plan.build(&output).unwrap();
        assert!(verify(&output).is_ok());
        assert!(plan.build(&output).is_err());
        assert!(plan.build(&root.join("input/nested")).is_err());
        fs::write(output.join("extra.dll"), b"untracked").unwrap();
        assert!(verify(&output).is_err());
        fs::remove_file(output.join("extra.dll")).unwrap();
        fs::write(output.join("newviso.exe"), b"corrupt").unwrap();
        assert!(verify(&output).is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn package_paths_reject_traversal_and_windows_prefixes() {
        for p in [
            "../outside",
            "/root",
            "C:/root",
            "a/../outside",
            "a\\outside",
            "",
        ] {
            assert!(safe_relative(p).is_err(), "{p}");
        }
        assert!(safe_relative("game/scenes/main.json").is_ok());
    }
}
