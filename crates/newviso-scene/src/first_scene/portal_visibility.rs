use super::*;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Clone, Debug)]
struct PortalRoom {
    id: u32,
    bounds: SceneBounds,
    camera_volume: bool,
}

#[derive(Clone, Debug)]
struct ScenePortal {
    from: u32,
    to: u32,
    _corners: Vec<Vec3>,
}

#[derive(Clone, Debug)]
pub(super) struct PortalVisibilityGraph {
    max_depth: u32,
    rooms: Vec<PortalRoom>,
    portals: Vec<ScenePortal>,
    entity_room_names: BTreeMap<String, u32>,
    entity_rooms: BTreeMap<u64, u32>,
    adjacency: BTreeMap<u32, Vec<usize>>,
    last_visible_rooms: BTreeSet<u32>,
}

impl PortalVisibilityGraph {
    pub(super) fn from_scene_value(scene: &Value) -> Result<Option<Self>, String> {
        let Some(value) = scene.get("visibility_graph") else {
            return Ok(None);
        };
        if value.get("schema").and_then(Value::as_str) != Some("newviso.portal_visibility.v1") {
            return Err("unsupported scene visibility_graph schema".to_owned());
        }
        let max_depth = value
            .get("max_depth")
            .and_then(Value::as_u64)
            .and_then(|v| u32::try_from(v).ok())
            .unwrap_or(8)
            .clamp(1, 32);

        let mut rooms = Vec::new();
        for room in value
            .get("rooms")
            .and_then(Value::as_array)
            .ok_or("visibility_graph.rooms must be an array")?
        {
            let id = room
                .get("id")
                .and_then(Value::as_u64)
                .and_then(|v| u32::try_from(v).ok())
                .ok_or("visibility room requires u32 id")?;
            let bounds = room
                .get("bounds")
                .ok_or("visibility room requires bounds")?;
            let min = read_vec3(bounds, "min", Vec3::ZERO)?;
            let max = read_vec3(bounds, "max", Vec3::ZERO)?;
            if min.x > max.x || min.y > max.y || min.z > max.z {
                return Err(format!("visibility room {id} has invalid bounds"));
            }
            rooms.push(PortalRoom {
                id,
                bounds: SceneBounds { min, max },
                camera_volume: room
                    .get("camera_volume")
                    .and_then(Value::as_bool)
                    .unwrap_or(true),
            });
        }

        let mut portals = Vec::new();
        for portal in value
            .get("portals")
            .and_then(Value::as_array)
            .ok_or("visibility_graph.portals must be an array")?
        {
            let from = portal
                .get("from")
                .and_then(Value::as_u64)
                .and_then(|v| u32::try_from(v).ok())
                .ok_or("portal requires u32 from")?;
            let to = portal
                .get("to")
                .and_then(Value::as_u64)
                .and_then(|v| u32::try_from(v).ok())
                .ok_or("portal requires u32 to")?;
            let corners = portal
                .get("corners")
                .and_then(Value::as_array)
                .ok_or("portal requires corners")?
                .iter()
                .map(|corner| {
                    let values = corner.as_array().ok_or("portal corner must be vec3")?;
                    if values.len() != 3 {
                        return Err("portal corner must contain 3 values".to_owned());
                    }
                    let mut out = [0.0f32; 3];
                    for (index, target) in out.iter_mut().enumerate() {
                        *target = values[index]
                            .as_f64()
                            .map(|v| v as f32)
                            .filter(|v| v.is_finite())
                            .ok_or("portal corner must contain finite numbers")?;
                    }
                    Ok(Vec3::new(out[0], out[1], out[2]))
                })
                .collect::<Result<Vec<_>, String>>()?;
            if corners.len() < 3 {
                return Err("portal requires at least 3 corners".to_owned());
            }
            portals.push(ScenePortal {
                from,
                to,
                _corners: corners,
            });
        }

        let entity_room_names = value
            .get("entity_rooms")
            .and_then(Value::as_object)
            .ok_or("visibility_graph.entity_rooms must be an object")?
            .iter()
            .map(|(name, room)| {
                let room = room
                    .as_u64()
                    .and_then(|v| u32::try_from(v).ok())
                    .ok_or_else(|| format!("entity room '{name}' must be u32"))?;
                Ok((name.clone(), room))
            })
            .collect::<Result<BTreeMap<_, _>, String>>()?;

        let mut adjacency = BTreeMap::<u32, Vec<usize>>::new();
        for (index, portal) in portals.iter().enumerate() {
            adjacency.entry(portal.from).or_default().push(index);
            adjacency.entry(portal.to).or_default().push(index);
        }

        Ok(Some(Self {
            max_depth,
            rooms,
            portals,
            entity_room_names,
            entity_rooms: BTreeMap::new(),
            adjacency,
            last_visible_rooms: BTreeSet::new(),
        }))
    }

    pub(super) fn bind_entities(&mut self, world: &SceneWorld) {
        self.entity_rooms.clear();
        for (name, room) in &self.entity_room_names {
            if let Some(id) = world.entity_id_by_name(name) {
                self.entity_rooms.insert(id.0, *room);
            }
        }
    }

    fn camera_rooms(&self, position: Vec3) -> BTreeSet<u32> {
        self.rooms
            .iter()
            .filter(|room| room.camera_volume && point_in_bounds(position, room.bounds))
            .map(|room| room.id)
            .collect()
    }

    fn visible_rooms(&self, camera: &Camera) -> Option<BTreeSet<u32>> {
        let starts = self.camera_rooms(camera.position);
        if starts.is_empty() {
            return None;
        }

        // The imported RSC7 MLO room bounds are conservative world-space AABBs.
        // Several rooms can overlap after rotation, and imported portal polygons are
        // not precise enough to be an authoritative screen-space visibility mask.
        //
        // Keep the room graph as a coarse connectivity/PVS filter only. Camera
        // orientation is handled later by the normal scene frustum and GPU Hi-Z.
        // This deliberately prevents a portal projection error from making visible
        // geometry disappear while the player rotates the camera.
        let mut visible = starts.clone();
        let mut queue = starts
            .into_iter()
            .map(|room| (room, 0u32))
            .collect::<VecDeque<_>>();

        while let Some((room, depth)) = queue.pop_front() {
            if depth >= self.max_depth {
                continue;
            }
            let Some(portals) = self.adjacency.get(&room) else {
                continue;
            };
            for &portal_index in portals {
                let portal = &self.portals[portal_index];
                let next = if portal.from == room {
                    portal.to
                } else if portal.to == room {
                    portal.from
                } else {
                    continue;
                };
                if visible.insert(next) {
                    queue.push_back((next, depth + 1));
                }
            }
        }
        Some(visible)
    }

    pub(super) fn filter_candidates(
        &mut self,
        camera: &Camera,
        _aspect: f32,
        candidates: &mut BTreeSet<SceneEntityId>,
    ) {
        let Some(visible_rooms) = self.visible_rooms(camera) else {
            self.last_visible_rooms.clear();
            return; // fail open outside the authored room graph
        };
        self.last_visible_rooms = visible_rooms.clone();
        candidates.retain(|id| {
            self.entity_rooms
                .get(&id.0)
                .is_none_or(|room| visible_rooms.contains(room))
        });
    }

    pub(super) fn telemetry(&self) -> Value {
        json!({
            "mode": "connectivity_conservative",
            "rooms": self.rooms.len(),
            "portals": self.portals.len(),
            "mapped_entities": self.entity_rooms.len(),
            "visible_rooms": self.last_visible_rooms.iter().copied().collect::<Vec<_>>(),
        })
    }
}

fn point_in_bounds(point: Vec3, bounds: SceneBounds) -> bool {
    point.x >= bounds.min.x
        && point.x <= bounds.max.x
        && point.y >= bounds.min.y
        && point.y <= bounds.max.y
        && point.z >= bounds.min.z
        && point.z <= bounds.max.z
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portal_visibility_filters_spatial_candidates_before_scene_scan() {
        let mut graph = PortalVisibilityGraph {
            max_depth: 8,
            rooms: vec![
                PortalRoom {
                    id: 1,
                    bounds: SceneBounds {
                        min: Vec3::new(-10.0, -10.0, -10.0),
                        max: Vec3::new(10.0, 10.0, 10.0),
                    },
                    camera_volume: true,
                },
                PortalRoom {
                    id: 2,
                    bounds: SceneBounds {
                        min: Vec3::new(20.0, -10.0, -10.0),
                        max: Vec3::new(40.0, 10.0, 10.0),
                    },
                    camera_volume: true,
                },
            ],
            portals: Vec::new(),
            entity_room_names: BTreeMap::new(),
            entity_rooms: BTreeMap::from([(1, 1), (2, 2)]),
            adjacency: BTreeMap::new(),
            last_visible_rooms: BTreeSet::new(),
        };
        let camera = Camera {
            position: Vec3::ZERO,
            target: Vec3::new(0.0, 0.0, -1.0),
            up: Vec3::Y,
            fov_y_degrees: 70.0,
            near: 0.1,
            far: 100.0,
        };
        let mut candidates = BTreeSet::from([SceneEntityId(1), SceneEntityId(2), SceneEntityId(3)]);

        graph.filter_candidates(&camera, 16.0 / 9.0, &mut candidates);

        assert_eq!(
            candidates,
            BTreeSet::from([SceneEntityId(1), SceneEntityId(3)])
        );
        assert_eq!(graph.last_visible_rooms, BTreeSet::from([1]));
    }

    #[test]
    fn portal_connectivity_does_not_change_when_camera_rotates() {
        let mut graph = PortalVisibilityGraph {
            max_depth: 8,
            rooms: vec![
                PortalRoom {
                    id: 1,
                    bounds: SceneBounds {
                        min: Vec3::new(-10.0, -10.0, -10.0),
                        max: Vec3::new(10.0, 10.0, 10.0),
                    },
                    camera_volume: true,
                },
                PortalRoom {
                    id: 2,
                    bounds: SceneBounds {
                        min: Vec3::new(20.0, -10.0, -10.0),
                        max: Vec3::new(40.0, 10.0, 10.0),
                    },
                    camera_volume: true,
                },
            ],
            portals: vec![ScenePortal {
                from: 1,
                to: 2,
                _corners: vec![
                    Vec3::new(10.0, -1.0, -1.0),
                    Vec3::new(10.0, 1.0, -1.0),
                    Vec3::new(10.0, 1.0, 1.0),
                    Vec3::new(10.0, -1.0, 1.0),
                ],
            }],
            entity_room_names: BTreeMap::new(),
            entity_rooms: BTreeMap::from([(1, 1), (2, 2)]),
            adjacency: BTreeMap::from([(1, vec![0]), (2, vec![0])]),
            last_visible_rooms: BTreeSet::new(),
        };
        let mut candidates = BTreeSet::from([SceneEntityId(1), SceneEntityId(2)]);
        let forward_camera = Camera {
            position: Vec3::ZERO,
            target: Vec3::new(1.0, 0.0, 0.0),
            up: Vec3::Y,
            fov_y_degrees: 70.0,
            near: 0.1,
            far: 100.0,
        };
        graph.filter_candidates(&forward_camera, 16.0 / 9.0, &mut candidates);
        assert_eq!(
            candidates,
            BTreeSet::from([SceneEntityId(1), SceneEntityId(2)])
        );

        let mut candidates = BTreeSet::from([SceneEntityId(1), SceneEntityId(2)]);
        let reverse_camera = Camera {
            target: Vec3::new(-1.0, 0.0, 0.0),
            ..forward_camera
        };
        graph.filter_candidates(&reverse_camera, 16.0 / 9.0, &mut candidates);
        assert_eq!(
            candidates,
            BTreeSet::from([SceneEntityId(1), SceneEntityId(2)])
        );
    }
}
