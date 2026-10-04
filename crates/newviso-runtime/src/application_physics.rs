use super::*;

const STATIC_COLLIDER_ID_BASE: u64 = 1_u64 << 63;
const MIN_PHYSICS_HZ: f32 = 10.0;
const MAX_PHYSICS_HZ: f32 = 1000.0;
const MAX_PHYSICS_STEPS_LIMIT: usize = 64;
const SCENE_COLLIDER_BROADPHASE_MARGIN: f32 = 32.0;
const CONTACT_SURFACE_MAX_DISTANCE: f32 = 0.35;

#[derive(Clone, Copy, Debug)]
struct PhysicsWorldSettings {
    fixed_hz: f32,
    max_steps_per_frame: usize,
    gravity: f32,
    contact_skin: f32,
    scene_colliders_enabled: bool,
    scene_material: PhysicsMaterial,
    scene_participates_in_queries: bool,
    scene_casts_contacts: bool,
}

impl Default for PhysicsWorldSettings {
    fn default() -> Self {
        Self {
            fixed_hz: 60.0,
            max_steps_per_frame: 8,
            gravity: 0.0,
            contact_skin: 0.002,
            scene_colliders_enabled: false,
            scene_material: PhysicsMaterial {
                friction: 0.5,
                restitution: 0.0,
                density: 1.0,
            },
            scene_participates_in_queries: true,
            scene_casts_contacts: true,
        }
    }
}

impl PhysicsWorldSettings {
    fn fixed_dt(self) -> f32 {
        1.0 / self.fixed_hz
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PhysicsDamageKind {
    Collision,
    Bullet,
    Explosive,
    Fire,
    Melee,
    Water,
    Script,
}

impl PhysicsDamageKind {
    pub(super) fn parse(value: Option<&str>) -> Result<Self, String> {
        match value
            .unwrap_or("script")
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "collision" | "impact" => Ok(Self::Collision),
            "bullet" | "ballistic" | "weapon" => Ok(Self::Bullet),
            "explosive" | "explosion" => Ok(Self::Explosive),
            "fire" | "burn" => Ok(Self::Fire),
            "melee" => Ok(Self::Melee),
            "water" | "drown" => Ok(Self::Water),
            "script" | "direct" => Ok(Self::Script),
            other => Err(format!("unsupported physics damage kind '{other}'")),
        }
    }

    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Collision => "collision",
            Self::Bullet => "bullet",
            Self::Explosive => "explosive",
            Self::Fire => "fire",
            Self::Melee => "melee",
            Self::Water => "water",
            Self::Script => "script",
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct PhysicsDamageSource {
    damage: f32,
    kind: PhysicsDamageKind,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct PhysicsDamageContact {
    pub source: u64,
    pub target: u64,
    pub damage_kind: PhysicsDamageKind,
    pub direct_damage: f32,
    pub contact_impulse: f32,
    pub point: [f32; 3],
    pub impulse_direction: [f32; 3],
}

#[derive(Clone, Copy, Debug)]
struct PendingBallisticDamage {
    source: u64,
    damage: f32,
    damage_kind: PhysicsDamageKind,
    impulse: f32,
    direction: [f32; 3],
    falloff_min: f32,
    falloff_max: f32,
    falloff_modifier: f32,
}

#[derive(Clone, Copy, Debug)]
struct PhysicsBallisticImpact {
    source: u64,
    target: u64,
    point: [f32; 3],
    normal: [f32; 3],
    distance: f32,
    surface_entity: Option<u64>,
    surface_id: Option<u32>,
}

pub(super) struct PreparedStreamedCollision {
    entity: u64,
    collider: MeshCollider,
    sweep_mesh: newviso_collision::SphereSweepMesh,
    surface_ids: Vec<u32>,
    position: [f32; 3],
    bounds_min: [f32; 3],
    bounds_max: [f32; 3],
    nav_source: newviso_navigation::NavTileSource,
}

#[derive(Debug)]
pub(super) struct PhysicsRuntime {
    client: PhysicsClient,
    bodies: BTreeMap<u64, PhysicsBodySnapshot>,
    streamed_colliders: BTreeMap<u64, PhysicsFrameColliderSnapshot>,
    camera_collision_meshes: BTreeMap<u64, newviso_collision::SphereSweepMesh>,
    streamed_surface_ids: BTreeMap<u64, Vec<u32>>,
    pending_streamed_colliders: BTreeMap<u64, PhysicsFrameColliderSnapshot>,
    pending_commands: Vec<PhysicsCommand>,
    pending_queries: Vec<PhysicsQuery>,
    pending_ballistic_damage: BTreeMap<u64, PendingBallisticDamage>,
    ballistic_damage_contacts: Vec<PhysicsDamageContact>,
    ballistic_impacts: Vec<PhysicsBallisticImpact>,
    damage_sources: BTreeMap<u64, PhysicsDamageSource>,
    scene_pose_offsets: BTreeMap<u64, [f32; 3]>,
    scene_rotation_offsets: BTreeMap<u64, [f32; 4]>,
    frame_index: u64,
    fixed_tick: u64,
    next_command_seq: u64,
    accumulator: f32,
    settings: PhysicsWorldSettings,
    last_output: PhysicsFrameOutput,
}

mod colliders;
mod commands;
mod damage;
mod math;
mod presentation;
mod queries;
mod step;
use math::*;

impl PhysicsRuntime {
    #[cfg(test)]
    pub(super) fn empty_for_tests() -> Self {
        Self {
            client: PhysicsClient::new(),
            bodies: BTreeMap::new(),
            streamed_colliders: BTreeMap::new(),
            camera_collision_meshes: BTreeMap::new(),
            streamed_surface_ids: BTreeMap::new(),
            pending_streamed_colliders: BTreeMap::new(),
            pending_commands: Vec::new(),
            pending_queries: Vec::new(),
            pending_ballistic_damage: BTreeMap::new(),
            ballistic_damage_contacts: Vec::new(),
            ballistic_impacts: Vec::new(),
            damage_sources: BTreeMap::new(),
            scene_pose_offsets: BTreeMap::new(),
            scene_rotation_offsets: BTreeMap::new(),
            frame_index: 0,
            fixed_tick: 0,
            next_command_seq: 1,
            accumulator: 0.0,
            settings: PhysicsWorldSettings::default(),
            last_output: PhysicsFrameOutput::default(),
        }
    }

    pub(super) fn connect() -> Result<Self, String> {
        let client = PhysicsClient::new();
        let negotiation = client.negotiate(
            vec![
                PhysicsFeature::StaticColliders,
                PhysicsFeature::DynamicBodies,
                PhysicsFeature::MeshColliders,
            ],
            vec![
                PhysicsFeature::Contacts,
                PhysicsFeature::Queries,
                PhysicsFeature::NativeBackend,
            ],
        )?;

        host::info(
            "newviso.physics",
            format!(
                "physics runtime connected version={}.{}.{} features={:?}",
                negotiation.backend_version.major,
                negotiation.backend_version.minor,
                negotiation.backend_version.patch,
                negotiation.enabled_features
            ),
        );

        Ok(Self {
            client,
            bodies: BTreeMap::new(),
            streamed_colliders: BTreeMap::new(),
            camera_collision_meshes: BTreeMap::new(),
            streamed_surface_ids: BTreeMap::new(),
            pending_streamed_colliders: BTreeMap::new(),
            pending_commands: Vec::new(),
            pending_queries: Vec::new(),
            pending_ballistic_damage: BTreeMap::new(),
            ballistic_damage_contacts: Vec::new(),
            ballistic_impacts: Vec::new(),
            damage_sources: BTreeMap::new(),
            scene_pose_offsets: BTreeMap::new(),
            scene_rotation_offsets: BTreeMap::new(),
            frame_index: 0,
            fixed_tick: 0,
            next_command_seq: 1,
            accumulator: 0.0,
            settings: PhysicsWorldSettings::default(),
            last_output: PhysicsFrameOutput::default(),
        })
    }
}

fn command_u64(value: &Value, key: &str, index: usize) -> Result<u64, String> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("script command[{index}] requires unsigned integer '{key}'"))
}

fn optional_number(value: &Value, key: &str, default: f32) -> Result<f32, String> {
    match value.get(key) {
        Some(value) => value
            .as_f64()
            .map(|value| value as f32)
            .filter(|value| value.is_finite())
            .ok_or_else(|| format!("physics field '{key}' must be finite numeric")),
        None => Ok(default),
    }
}

fn optional_nullable_number(value: &Value, key: &str) -> Result<Option<f32>, String> {
    match value.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_f64()
            .map(|value| value as f32)
            .filter(|value| value.is_finite())
            .map(Some)
            .ok_or_else(|| format!("physics field '{key}' must be finite numeric or null")),
    }
}

fn optional_bool(value: &Value, key: &str, default: bool) -> Result<bool, String> {
    match value.get(key) {
        Some(value) => value
            .as_bool()
            .ok_or_else(|| format!("physics field '{key}' must be boolean")),
        None => Ok(default),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use newviso_physics_client::{PhysicsContactEvent, PhysicsContactMaterialPair};

    #[test]
    fn failed_provider_submission_restores_collision_command_and_query_buffers() {
        let mut runtime = PhysicsRuntime::empty_for_tests();
        let collision = CollisionMeshResource {
            id: newviso_resource_runtime::AssetId(91),
            name: "retry.ground".to_owned(),
            bounds: newviso_collision::CollisionBounds {
                min: [-1.0, 0.0, -1.0],
                max: [1.0, 0.0, 1.0],
            },
            vertices: vec![[-1.0, 0.0, -1.0], [1.0, 0.0, -1.0], [0.0, 0.0, 1.0]],
            triangles: vec![[0, 2, 1]],
            material_indices: vec![0],
        };
        runtime
            .install_streamed_collision(42, &collision, [0.0; 3], [0.0; 3], [1.0; 3])
            .unwrap();
        runtime.pending_commands.push(PhysicsCommand {
            seq: 17,
            kind: PhysicsCommandKind::DestroyBody { entity: 7 },
        });
        runtime.pending_queries.push(PhysicsQuery {
            seq: 18,
            ignore_entity: None,
            kind: PhysicsQueryKind::Ray {
                origin: [0.0, 2.0, 0.0],
                dir: [0.0, -1.0, 0.0],
                max_t: 5.0,
            },
        });
        let colliders = runtime.pending_streamed_colliders.clone();
        let commands = runtime.pending_commands.clone();
        let queries = runtime.pending_queries.clone();
        let mut vehicles = VehicleRuntime::default();
        assert!(runtime.step(1.0 / 60.0, &[], &mut vehicles).is_err());
        assert_eq!(runtime.pending_streamed_colliders, colliders);
        assert_eq!(runtime.pending_commands, commands);
        assert_eq!(runtime.pending_queries, queries);
    }

    #[test]
    fn streamed_mesh_colliders_are_sent_once_then_persist_backend_side() {
        // Lifecycle invariant: large static geometry is staged for one packet
        // after install/change, then omitted from steady-state frame packets.
        let mut runtime = PhysicsRuntime {
            client: PhysicsClient::new(),
            bodies: BTreeMap::new(),
            streamed_colliders: BTreeMap::new(),
            camera_collision_meshes: BTreeMap::new(),
            streamed_surface_ids: BTreeMap::new(),
            pending_streamed_colliders: BTreeMap::new(),
            pending_commands: Vec::new(),
            pending_queries: Vec::new(),
            pending_ballistic_damage: BTreeMap::new(),
            ballistic_damage_contacts: Vec::new(),
            ballistic_impacts: Vec::new(),
            damage_sources: BTreeMap::new(),
            scene_pose_offsets: BTreeMap::new(),
            scene_rotation_offsets: BTreeMap::new(),
            frame_index: 0,
            fixed_tick: 0,
            next_command_seq: 1,
            accumulator: 0.0,
            settings: PhysicsWorldSettings::default(),
            last_output: PhysicsFrameOutput::default(),
        };
        let collision = CollisionMeshResource {
            id: newviso_resource_runtime::AssetId(1),
            name: "ground".to_owned(),
            bounds: newviso_collision::CollisionBounds {
                min: [-1.0, 0.0, -1.0],
                max: [1.0, 0.0, 1.0],
            },
            vertices: vec![
                [-1.0, 0.0, -1.0],
                [1.0, 0.0, -1.0],
                [1.0, 0.0, 1.0],
                [-1.0, 0.0, 1.0],
            ],
            triangles: vec![[0, 2, 1], [0, 3, 2]],
            material_indices: vec![0, 0],
        };
        runtime
            .install_streamed_collision(
                42,
                &collision,
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 0.0],
                [1.0, 1.0, 1.0],
            )
            .expect("install");
        assert_eq!(runtime.streamed_colliders.len(), 1);
        assert_eq!(runtime.pending_streamed_colliders.len(), 1);

        // Reinstalling byte-identical placement does not restage geometry.
        let changed = runtime
            .install_streamed_collision(
                42,
                &collision,
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 0.0],
                [1.0, 1.0, 1.0],
            )
            .expect("reinstall");
        assert!(!changed);
        assert_eq!(runtime.pending_streamed_colliders.len(), 1);

        assert!(runtime.remove_streamed_collision(42));
        assert!(runtime.pending_streamed_colliders.is_empty());
        assert!(runtime.pending_commands.iter().any(|command| {
            matches!(command.kind, PhysicsCommandKind::DestroyBody { entity: 42 })
        }));
    }

    #[test]
    fn contact_surface_is_resolved_from_authored_triangle_material() {
        let mut runtime = PhysicsRuntime {
            client: PhysicsClient::new(),
            bodies: BTreeMap::new(),
            streamed_colliders: BTreeMap::new(),
            camera_collision_meshes: BTreeMap::new(),
            streamed_surface_ids: BTreeMap::new(),
            pending_streamed_colliders: BTreeMap::new(),
            pending_commands: Vec::new(),
            pending_queries: Vec::new(),
            pending_ballistic_damage: BTreeMap::new(),
            ballistic_damage_contacts: Vec::new(),
            ballistic_impacts: Vec::new(),
            damage_sources: BTreeMap::new(),
            scene_pose_offsets: BTreeMap::new(),
            scene_rotation_offsets: BTreeMap::new(),
            frame_index: 0,
            fixed_tick: 0,
            next_command_seq: 1,
            accumulator: 0.0,
            settings: PhysicsWorldSettings::default(),
            last_output: PhysicsFrameOutput::default(),
        };
        let collision = CollisionMeshResource {
            id: newviso_resource_runtime::AssetId(77),
            name: "surface_test".to_owned(),
            bounds: newviso_collision::CollisionBounds {
                min: [-2.0, 0.0, -1.0],
                max: [2.0, 0.0, 1.0],
            },
            vertices: vec![
                [-2.0, 0.0, -1.0],
                [0.0, 0.0, -1.0],
                [-2.0, 0.0, 1.0],
                [0.0, 0.0, 1.0],
                [2.0, 0.0, -1.0],
                [2.0, 0.0, 1.0],
            ],
            triangles: vec![[0, 1, 2], [1, 3, 2], [1, 4, 3], [4, 5, 3]],
            material_indices: vec![1, 1, 55, 55],
        };
        runtime
            .install_streamed_collision(42, &collision, [10.0, 0.0, 0.0], [0.0; 3], [1.0; 3])
            .expect("install surface collider");

        let mut output = PhysicsFrameOutput::default();
        output
            .events
            .push(PhysicsEvent::ContactBegin(PhysicsContactEvent {
                a: 7,
                b: 42,
                point: [11.25, 0.02, 0.0],
                normal: [0.0, -1.0, 0.0],
                impulse: 1.0,
                relative_velocity: None,
                materials: PhysicsContactMaterialPair::default(),
                surface_id: None,
                surface_entity: None,
                surface_triangle: None,
            }));
        runtime.enrich_contact_surfaces(&mut output);

        let PhysicsEvent::ContactBegin(contact) = output.events[0] else {
            panic!("expected contact begin");
        };
        assert_eq!(contact.surface_id, Some(55));
        assert_eq!(contact.surface_entity, Some(42));
        assert!(matches!(contact.surface_triangle, Some(2 | 3)));
    }

    #[test]
    fn camera_sphere_sweep_stops_before_streamed_mesh_and_can_ignore_owner() {
        let mut runtime = PhysicsRuntime {
            client: PhysicsClient::new(),
            bodies: BTreeMap::new(),
            streamed_colliders: BTreeMap::new(),
            camera_collision_meshes: BTreeMap::new(),
            streamed_surface_ids: BTreeMap::new(),
            pending_streamed_colliders: BTreeMap::new(),
            pending_commands: Vec::new(),
            pending_queries: Vec::new(),
            pending_ballistic_damage: BTreeMap::new(),
            ballistic_damage_contacts: Vec::new(),
            ballistic_impacts: Vec::new(),
            damage_sources: BTreeMap::new(),
            scene_pose_offsets: BTreeMap::new(),
            scene_rotation_offsets: BTreeMap::new(),
            frame_index: 0,
            fixed_tick: 0,
            next_command_seq: 1,
            accumulator: 0.0,
            settings: PhysicsWorldSettings::default(),
            last_output: PhysicsFrameOutput::default(),
        };
        let wall = CollisionMeshResource {
            id: newviso_resource_runtime::AssetId(2),
            name: "camera_wall".to_owned(),
            bounds: newviso_collision::CollisionBounds {
                min: [-2.0, -2.0, 0.0],
                max: [2.0, 2.0, 0.0],
            },
            vertices: vec![
                [-2.0, -2.0, 0.0],
                [2.0, -2.0, 0.0],
                [2.0, 2.0, 0.0],
                [-2.0, 2.0, 0.0],
            ],
            triangles: vec![[0, 1, 2], [0, 2, 3]],
            material_indices: vec![0, 0],
        };
        runtime
            .install_streamed_collision(42, &wall, [0.0, 0.0, -2.0], [0.0; 3], [1.0; 3])
            .expect("install wall");

        let constrained =
            runtime.constrain_camera([0.0, 0.0, 0.0], [0.0, 0.0, -4.0], 0.25, None, &[]);
        assert!((constrained[2] + 1.74).abs() < 1.0e-4, "{constrained:?}");

        let ignored =
            runtime.constrain_camera([0.0, 0.0, 0.0], [0.0, 0.0, -4.0], 0.25, Some(42), &[]);
        assert_eq!(ignored, [0.0, 0.0, -4.0]);
    }

    #[test]
    fn scene_collider_interests_are_disabled_or_swept_around_dynamic_bodies() {
        let mut runtime = PhysicsRuntime {
            client: PhysicsClient::new(),
            bodies: BTreeMap::new(),
            streamed_colliders: BTreeMap::new(),
            camera_collision_meshes: BTreeMap::new(),
            streamed_surface_ids: BTreeMap::new(),
            pending_streamed_colliders: BTreeMap::new(),
            pending_commands: Vec::new(),
            pending_queries: Vec::new(),
            pending_ballistic_damage: BTreeMap::new(),
            ballistic_damage_contacts: Vec::new(),
            ballistic_impacts: Vec::new(),
            damage_sources: BTreeMap::new(),
            scene_pose_offsets: BTreeMap::new(),
            scene_rotation_offsets: BTreeMap::new(),
            frame_index: 0,
            fixed_tick: 0,
            next_command_seq: 1,
            accumulator: 0.0,
            settings: PhysicsWorldSettings::default(),
            last_output: PhysicsFrameOutput::default(),
        };
        assert!(runtime.scene_collider_interests().is_none());

        runtime.settings.scene_colliders_enabled = true;
        runtime.bodies.insert(
            99,
            PhysicsBodySnapshot {
                entity: 99,
                kind: PhysicsBodyKind::Dynamic,
                shape: CollisionShape::Box {
                    half_extents: [1.0, 1.0, 1.0],
                },
                flags: PhysicsBodyFlags {
                    is_trigger: false,
                    participates_in_queries: true,
                    casts_contacts: true,
                    continuous_collision: false,
                },
                material: PhysicsMaterial {
                    friction: 0.5,
                    restitution: 0.0,
                    density: 1.0,
                },
                position: [0.0, 0.0, 0.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
                linear_velocity: [12.0, 0.0, 0.0],
                angular_velocity: [0.0; 3],
                linear_damping: None,
                angular_damping: None,
                mass_properties: None,
                convex_hulls: Vec::new(),
                bounds_min: [-1.0, -1.0, -1.0],
                bounds_max: [1.0, 1.0, 1.0],
            },
        );

        let interests = runtime.scene_collider_interests().unwrap();
        assert_eq!(interests.len(), 1);
        assert!(interests[0].0[0] <= -33.0);
        assert!(interests[0].1[0] >= 34.5);
        assert!(interests[0].0[1] <= -33.0);
        assert!(interests[0].1[1] >= 33.0);
    }

    #[test]
    fn static_scene_body_uses_reserved_id_and_box_bounds() {
        let mut settings = PhysicsWorldSettings::default();
        settings.scene_colliders_enabled = true;
        let bodies = static_scene_bodies(&[(77, [-2.0, 0.0, -3.0], [2.0, 1.0, 3.0])], settings);
        assert_eq!(bodies.len(), 1);
        assert_eq!(bodies[0].entity, 77);
        assert_eq!(bodies[0].position, [0.0, 0.5, 0.0]);
        assert_eq!(
            bodies[0].shape,
            CollisionShape::Box {
                half_extents: [2.0, 0.5, 3.0]
            }
        );
    }

    #[test]
    fn physics_contacts_carry_projectile_damage_toward_scene_target() {
        let mut runtime = PhysicsRuntime::empty_for_tests();
        runtime.damage_sources.insert(
            1_000_001,
            PhysicsDamageSource {
                damage: 37.5,
                kind: PhysicsDamageKind::Bullet,
            },
        );
        runtime
            .last_output
            .events
            .push(PhysicsEvent::ContactBegin(PhysicsContactEvent {
                a: 1_000_001,
                b: 77,
                point: [2.0, 1.0, -3.0],
                normal: [0.0, 1.0, 0.0],
                impulse: 14.0,
                relative_velocity: Some([0.0, -100.0, 0.0]),
                materials: PhysicsContactMaterialPair::default(),
                surface_id: None,
                surface_entity: None,
                surface_triangle: None,
            }));

        let contacts = runtime.damage_contacts();
        assert_eq!(contacts.len(), 2);
        assert_eq!(contacts[0].source, 1_000_001);
        assert_eq!(contacts[0].target, 77);
        assert!((contacts[0].direct_damage - 37.5).abs() < 1.0e-6);
        assert!((contacts[0].contact_impulse - 14.0).abs() < 1.0e-6);
        assert_eq!(contacts[1].source, 77);
        assert_eq!(contacts[1].target, 1_000_001);
        assert_eq!(contacts[1].direct_damage, 0.0);
    }

    #[test]
    fn broken_scene_prop_promotes_to_dynamic_body_without_visual_pose_snap() {
        let mut runtime = PhysicsRuntime::empty_for_tests();
        let activation = SceneDestructionActivation {
            entity: 88,
            scene_position: [0.0, 0.0, 0.0],
            rotation_degrees: [0.0, 45.0, 0.0],
            bounds_min: [-0.25, 0.0, -0.25],
            bounds_max: [0.25, 6.0, 0.25],
            density: 40.0,
            friction: 0.65,
            restitution: 0.04,
            linear_damping: 0.08,
            angular_damping: 0.16,
            impulse_transfer: 0.9,
        };
        runtime
            .promote_scene_destructible(
                activation,
                PhysicsDamageContact {
                    source: 1_000_002,
                    target: 88,
                    damage_kind: PhysicsDamageKind::Bullet,
                    direct_damage: 55.0,
                    contact_impulse: 20.0,
                    point: [0.0, 1.0, 0.0],
                    impulse_direction: [1.0, 0.0, 0.0],
                },
            )
            .unwrap();

        let body = runtime.bodies.get(&88).expect("promoted body");
        assert_eq!(body.kind, PhysicsBodyKind::Dynamic);
        assert_eq!(body.position, [0.0, 3.0, 0.0]);
        assert_eq!(body.rotation, [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(runtime.scene_pose_offsets.get(&88), Some(&[0.0, -3.0, 0.0]));
        assert!(runtime.scene_rotation_offsets.contains_key(&88));
        assert!(runtime.pending_commands.iter().any(|command| {
            matches!(
                command.kind,
                PhysicsCommandKind::ApplyImpulse { entity: 88, .. }
            )
        }));
    }

    #[test]
    fn runtime_state_exposes_ballistic_damage_contacts_to_gameplay_scripts() {
        let mut runtime = PhysicsRuntime::empty_for_tests();
        runtime
            .ballistic_damage_contacts
            .push(PhysicsDamageContact {
                source: 900_000_100,
                target: 972_000_001,
                damage_kind: PhysicsDamageKind::Bullet,
                direct_damage: 32.0,
                contact_impulse: 75.0,
                point: [1.0, 2.0, 3.0],
                impulse_direction: [0.0, 0.0, -1.0],
            });
        let state = runtime.runtime_state();
        let contacts = state["damage_contacts"]
            .as_array()
            .expect("damage contacts array");
        assert_eq!(contacts.len(), 1);
        assert_eq!(contacts[0]["source"].as_u64(), Some(900_000_100));
        assert_eq!(contacts[0]["target"].as_u64(), Some(972_000_001));
        assert_eq!(contacts[0]["direct_damage"].as_f64(), Some(32.0));
    }

    #[test]
    fn runtime_state_predicts_presentation_pose_across_fixed_step_remainder() {
        let mut runtime = PhysicsRuntime::empty_for_tests();
        runtime.settings.fixed_hz = 120.0;
        runtime.accumulator = runtime.settings.fixed_dt() * 0.5;
        runtime.bodies.insert(
            7,
            PhysicsBodySnapshot {
                entity: 7,
                kind: PhysicsBodyKind::Dynamic,
                shape: CollisionShape::Capsule {
                    radius: 0.3,
                    half_height: 0.6,
                },
                flags: PhysicsBodyFlags {
                    is_trigger: false,
                    participates_in_queries: true,
                    casts_contacts: true,
                    continuous_collision: true,
                },
                material: PhysicsMaterial {
                    friction: 0.5,
                    restitution: 0.0,
                    density: 1.0,
                },
                position: [1.0, 2.0, 3.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
                linear_velocity: [12.0, -3.0, 6.0],
                angular_velocity: [0.0; 3],
                linear_damping: None,
                angular_damping: None,
                mass_properties: None,
                convex_hulls: Vec::new(),
                bounds_min: [0.7, 1.1, 2.7],
                bounds_max: [1.3, 2.9, 3.3],
            },
        );

        let state = runtime.runtime_state();
        let body = &state["bodies"][0];
        let presentation = body["presentation_position"]
            .as_array()
            .expect("presentation position array");
        let actual = body["position"]
            .as_array()
            .expect("authoritative position array");

        let value = |array: &Vec<Value>, index: usize| {
            array[index].as_f64().expect("finite numeric position")
        };
        assert!((value(actual, 0) - 1.0).abs() < 1.0e-6);
        assert!((value(presentation, 0) - 1.05).abs() < 1.0e-5);
        assert!((value(presentation, 1) - 1.9875).abs() < 1.0e-5);
        assert!((value(presentation, 2) - 3.025).abs() < 1.0e-5);
        assert!(
            (state["presentation_alpha"]
                .as_f64()
                .expect("presentation alpha")
                - 0.5)
                .abs()
                < 1.0e-6
        );
    }

    #[test]
    fn repeated_body_upsert_sends_pose_and_both_velocities_to_native_backend() {
        let mut runtime = PhysicsRuntime::empty_for_tests();
        let mut command = serde_json::json!({
            "entity":7,"shape":{"kind":"box","half_extents":[1.0,1.0,1.0]},
            "position":[0.0,2.0,0.0],
            "mass_properties":{"mass":1000.0,"center_of_mass":[0.0,0.0,0.0],"inertia_diagonal":[200.0,300.0,400.0]}
        });
        runtime.upsert_body_from_script(&command, 0).unwrap();
        assert!(runtime.pending_commands.is_empty());
        command["position"] = serde_json::json!([4.0, 3.0, 2.0]);
        command["linear_velocity"] = serde_json::json!([1.0, 0.0, 0.0]);
        command["angular_velocity"] = serde_json::json!([0.0, 0.0, 12.0]);
        runtime.upsert_body_from_script(&command, 0).unwrap();
        assert_eq!(runtime.pending_commands.len(), 3);
        assert!(matches!(
            runtime.pending_commands[0].kind,
            PhysicsCommandKind::SetBodyPose {
                position: [4.0, 3.0, 2.0],
                ..
            }
        ));
        assert!(matches!(
            runtime.pending_commands[1].kind,
            PhysicsCommandKind::SetAngularVelocity {
                velocity: [0.0, 0.0, 12.0],
                ..
            }
        ));
        assert!(matches!(
            runtime.pending_commands[2].kind,
            PhysicsCommandKind::SetLinearVelocity {
                velocity: [1.0, 0.0, 0.0],
                ..
            }
        ));
    }

    #[test]
    fn sphere_bounds_follow_physics_pose() {
        let (min, max) = shape_bounds(CollisionShape::Sphere { radius: 0.25 }, [1.0, 2.0, 3.0]);
        assert_eq!(min, [0.75, 1.75, 2.75]);
        assert_eq!(max, [1.25, 2.25, 3.25]);
    }
    #[test]
    fn detaching_collision_child_preserves_chassis_and_repair_restores_hulls() {
        let mut runtime = PhysicsRuntime::empty_for_tests();
        let chassis = vec![
            [-1.0, -0.3, -2.0],
            [1.0, -0.3, -2.0],
            [-1.0, 0.3, 2.0],
            [1.0, 0.3, 2.0],
        ];
        let bonnet = vec![
            [-0.7, 0.4, -1.8],
            [0.7, 0.4, -1.8],
            [-0.7, 0.5, -1.0],
            [0.7, 0.5, -1.0],
        ];
        runtime
            .upsert_body_from_script(
                &json!({"entity":7,"shape":{"kind":"box","half_extents":[1.0,0.5,2.0]},
            "convex_hulls":[chassis,bonnet],"position":[0.0,1.0,0.0]}),
                0,
            )
            .unwrap();
        let original = runtime.body_collision_hulls(7).unwrap();
        runtime.remove_fragment_collision_region(7, [0.0, 0.45, -1.4], [0.7, 0.05, 0.4]);
        assert_eq!(runtime.body_collision_hulls(7).unwrap().len(), 1);
        runtime.restore_body_collision_hulls(7, original);
        assert_eq!(runtime.body_collision_hulls(7).unwrap().len(), 2);
    }
}
