use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectWorldPersistence {
    /// Generated save state is project-local storage, separate from VFS assets.
    /// None disables persistence for this project.
    pub save_path: Option<String>,
    pub load_on_start: bool,
    pub save_on_shutdown: bool,
    /// Real runtime seconds; zero disables periodic autosaving.
    pub autosave_interval_seconds: f64,
}

impl Default for ProjectWorldPersistence {
    fn default() -> Self {
        Self {
            save_path: None,
            load_on_start: true,
            save_on_shutdown: true,
            autosave_interval_seconds: 0.0,
        }
    }
}

impl ProjectWorldPersistence {
    pub fn validate(&self) -> Result<(), String> {
        if !self.autosave_interval_seconds.is_finite()
            || !(0.0..=86_400.0).contains(&self.autosave_interval_seconds)
        {
            return Err("world persistence autosave interval is invalid".into());
        }
        if let Some(path) = &self.save_path {
            let normalized = path.replace('\\', "/");
            if normalized.trim().is_empty()
                || normalized.starts_with('/')
                || normalized.contains(':')
                || normalized
                    .split('/')
                    .any(|p| p.is_empty() || p == "." || p == "..")
            {
                return Err(
                    "world persistence save_path must be a project-relative file path".into(),
                );
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn save_paths_cannot_escape_project_root() {
        for path in [
            "../outside",
            "C:\\outside",
            "/outside",
            "save/../outside",
            "save\\..\\outside",
            "",
        ] {
            assert!(ProjectWorldPersistence {
                save_path: Some(path.into()),
                ..Default::default()
            }
            .validate()
            .is_err());
        }
        assert!(ProjectWorldPersistence {
            save_path: Some(".newviso/saves/world.json".into()),
            ..Default::default()
        }
        .validate()
        .is_ok());
    }
}
