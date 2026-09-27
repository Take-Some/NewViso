use super::*;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Clone, Copy, Debug)]
struct PortalClipRect {
    min_x: f32,
    min_y: f32,
    max_x: f32,
    max_y: f32,
}

impl PortalClipRect {
    const FULL: Self = Self {
        min_x: -1.0,
        min_y: -1.0,
        max_x: 1.0,
        max_y: 1.0,
    };

    fn intersect(self, other: Self) -> Option<Self> {
        let rect = Self {
            min_x: self.min_x.max(other.min_x),
            min_y: self.min_y.max(other.min_y),
            max_x: self.max_x.min(other.max_x),
            max_y: self.max_y.min(other.max_y),
        };
        (rect.min_x <= rect.max_x && rect.min_y <= rect.max_y).then_some(rect)
    }
}

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
    corners: Vec<Vec3>,
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
            portals.push(ScenePortal { from, to, corners });
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

    fn camera_room(&self, position: Vec3) -> Option<u32> {
        self.rooms
            .iter()
            .filter(|room| room.camera_volume && point_in_bounds(position, room.bounds))
            .min_by(|a, b| {
                bounds_volume(a.bounds)
                    .partial_cmp(&bounds_volume(b.bounds))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|room| room.id)
    }

    fn visible_rooms(&self, camera: &Camera, aspect: f32) -> Option<BTreeSet<u32>> {
        let start = self.camera_room(camera.position)?;
        let matrix = camera_view_projection(camera, aspect);
        let mut visible = BTreeSet::from([start]);
        let mut queue = VecDeque::from([(start, PortalClipRect::FULL, 0u32)]);

        while let Some((room, clip, depth)) = queue.pop_front() {
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
                if visible.contains(&next) {
                    continue;
                }
                let Some(portal_rect) = project_portal_rect(&portal.corners, matrix) else {
                    continue;
                };
                let Some(next_clip) = clip.intersect(portal_rect) else {
                    continue;
                };
                visible.insert(next);
                queue.push_back((next, next_clip, depth + 1));
            }
        }
        Some(visible)
    }

    pub(super) fn filter_candidates(
        &mut self,
        camera: &Camera,
        aspect: f32,
        candidates: &mut BTreeSet<SceneEntityId>,
    ) {
        let Some(visible_rooms) = self.visible_rooms(camera, aspect) else {
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

fn bounds_volume(bounds: SceneBounds) -> f32 {
    let d = bounds.max.sub(bounds.min);
    (d.x.abs() * d.y.abs() * d.z.abs()).max(0.0)
}

fn project_portal_rect(corners: &[Vec3], matrix: [f32; 16]) -> Option<PortalClipRect> {
    let mut projected = Vec::<[f32; 2]>::new();
    let mut any_behind = false;
    for corner in corners {
        let clip = mul_mat4_vec4(matrix, [corner.x, corner.y, corner.z, 1.0]);
        if clip[3] <= 1.0e-5 {
            any_behind = true;
            continue;
        }
        projected.push([clip[0] / clip[3], clip[1] / clip[3]]);
    }
    if projected.is_empty() {
        return None;
    }
    if any_behind {
        // Portal intersects the near plane. Fail open for this portal rather
        // than accidentally culling a room the camera is entering.
        return Some(PortalClipRect::FULL);
    }

    let min_x = projected.iter().map(|p| p[0]).fold(f32::INFINITY, f32::min);
    let min_y = projected.iter().map(|p| p[1]).fold(f32::INFINITY, f32::min);
    let max_x = projected
        .iter()
        .map(|p| p[0])
        .fold(f32::NEG_INFINITY, f32::max);
    let max_y = projected
        .iter()
        .map(|p| p[1])
        .fold(f32::NEG_INFINITY, f32::max);
    PortalClipRect {
        min_x,
        min_y,
        max_x,
        max_y,
    }
    .intersect(PortalClipRect::FULL)
}

fn mul_mat4_vec4(matrix: [f32; 16], vector: [f32; 4]) -> [f32; 4] {
    [
        matrix[0] * vector[0]
            + matrix[4] * vector[1]
            + matrix[8] * vector[2]
            + matrix[12] * vector[3],
        matrix[1] * vector[0]
            + matrix[5] * vector[1]
            + matrix[9] * vector[2]
            + matrix[13] * vector[3],
        matrix[2] * vector[0]
            + matrix[6] * vector[1]
            + matrix[10] * vector[2]
            + matrix[14] * vector[3],
        matrix[3] * vector[0]
            + matrix[7] * vector[1]
            + matrix[11] * vector[2]
            + matrix[15] * vector[3],
    ]
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
    fn clip_rect_intersection_rejects_disjoint_portals() {
        let a = PortalClipRect {
            min_x: -1.0,
            min_y: -1.0,
            max_x: -0.5,
            max_y: 1.0,
        };
        let b = PortalClipRect {
            min_x: 0.5,
            min_y: -1.0,
            max_x: 1.0,
            max_y: 1.0,
        };
        assert!(a.intersect(b).is_none());
    }
}
