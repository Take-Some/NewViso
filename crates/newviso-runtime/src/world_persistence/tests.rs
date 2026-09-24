use super::*;

struct ProjectDir(PathBuf);
impl ProjectDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "newviso-world-save-test-{}-{}",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for ProjectDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn settings() -> ProjectWorldPersistence {
    ProjectWorldPersistence {
        save_path: Some("saves/world.json".into()),
        ..Default::default()
    }
}
fn binding() -> WorldActorPresentationBinding {
    WorldActorPresentationBinding {
        scene_key: "actor.visual".into(),
        visual: SceneRuntimeVisualKind::Cube,
        asset_ref: None,
        position_offset: [0.0; 3],
        rotation_degrees: [0.0; 3],
        scale: [1.0; 3],
        bounds_half_extent: [0.5; 3],
        base_color: [1.0; 4],
        solid: false,
        visible_distance: f32::INFINITY,
        stream_distance: f32::INFINITY,
        fade_range: 0.0,
        materialized_representations: vec!["physical".into(), "proxy".into()],
    }
}
#[test]
fn save_restart_and_corrupt_primary_recovery_keep_valid_state() {
    let dir = ProjectDir::new();
    let mut startup = WorldStartup::open(&dir.0, "test", &settings()).unwrap();
    startup.world.set_fact("stock", json!(4)).unwrap();
    startup.presentations.insert("actor".into(), binding());
    let store = startup.persistence.as_mut().unwrap();
    store.save(&startup.world, &startup.presentations).unwrap();
    startup.world.tick_frame(0.2, &[]);
    startup.world.set_fact("stock", json!(8)).unwrap();
    store.save(&startup.world, &startup.presentations).unwrap();
    let resumed = WorldStartup::open(&dir.0, "test", &settings()).unwrap();
    assert_eq!(
        resumed.world.checkpoint().unwrap(),
        startup.world.checkpoint().unwrap()
    );
    assert_eq!(
        resumed.presentations["actor"].visible_distance,
        f32::INFINITY
    );
    assert!(resumed.persistence.as_ref().unwrap().restored);
    fs::write(&store.path, b"interrupted/corrupt bytes").unwrap();
    let mut recovered = WorldStartup::open(&dir.0, "test", &settings()).unwrap();
    assert!(recovered.persistence.as_ref().unwrap().recovered_backup);
    assert_eq!(recovered.world.runtime_state()["facts"][0]["value"], 4);
    recovered
        .persistence
        .as_mut()
        .unwrap()
        .save(&recovered.world, &recovered.presentations)
        .unwrap();
    assert!(WorldStartup::open(&dir.0, "test", &settings()).is_ok());
    assert!(WorldStartup::open(&dir.0, "another-project", &settings()).is_err());
}
#[test]
fn invalid_saves_fail_without_overwriting_evidence() {
    let dir = ProjectDir::new();
    fs::create_dir_all(dir.0.join("saves")).unwrap();
    let path = dir.0.join("saves/world.json");
    fs::write(&path, b"broken").unwrap();
    assert!(WorldStartup::open(&dir.0, "test", &settings()).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"broken");
}
