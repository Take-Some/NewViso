use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ContactTrackKind {
    Tyre,
    Rim,
}

#[derive(Clone, Copy)]
struct Anchor {
    point: [f32; 3],
    normal: [f32; 3],
    surface: Option<u64>,
    material: Option<u32>,
    intensity: f32,
    kind: ContactTrackKind,
}

#[derive(Default)]
pub(super) struct VehicleTracks {
    anchors: BTreeMap<(u64, usize), Anchor>,
    expires: BTreeMap<usize, f64>,
    next: usize,
    allocated: usize,
    pub emitted: u64,
    pub rim_marks_emitted: u64,
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    (0..3).map(|i| a[i] * b[i]).sum()
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn unit(a: [f32; 3]) -> Option<[f32; 3]> {
    let n = dot(a, a).sqrt();
    (n > 1e-5 && n.is_finite()).then(|| a.map(|x| x / n))
}

fn lerp_anchor(a: Anchor, b: Anchor, t: f32) -> Anchor {
    let t = t.clamp(0.0, 1.0);
    Anchor {
        point: std::array::from_fn(|i| a.point[i] + (b.point[i] - a.point[i]) * t),
        normal: unit(std::array::from_fn(|i| {
            a.normal[i] + (b.normal[i] - a.normal[i]) * t
        }))
        .unwrap_or(a.normal),
        surface: b.surface,
        material: b.material,
        intensity: a.intensity + (b.intensity - a.intensity) * t,
        kind: b.kind,
    }
}

// A unit XZ plane follows the contact tangent and normal, including banked roads.
fn ribbon_pose(a: Anchor, b: Anchor, gap: f32) -> Option<([f32; 3], [f32; 3], f32)> {
    if a.surface != b.surface
        || a.material != b.material
        || a.kind != b.kind
        || dot(a.normal, b.normal) < 0.9
    {
        return None;
    }
    let delta: [f32; 3] = std::array::from_fn(|i| b.point[i] - a.point[i]);
    let distance = dot(delta, delta).sqrt();
    if distance > gap {
        return None;
    }
    let n = unit(std::array::from_fn(|i| a.normal[i] + b.normal[i]))?;
    let z = unit(std::array::from_fn(|i| delta[i] - n[i] * dot(delta, n)))?;
    let x = unit(cross(n, z))?;
    let y = cross(z, x);
    let rotation = [
        y[2].atan2(z[2]).to_degrees(),
        (-x[2]).clamp(-1.0, 1.0).asin().to_degrees(),
        x[1].atan2(x[0]).to_degrees(),
    ];
    let center = std::array::from_fn(|i| (a.point[i] + b.point[i]) * 0.5 + n[i] * 0.012);
    Some((center, rotation, distance))
}

impl EngineApplication {
    pub(super) fn clear_vehicle_tracks(&mut self) -> Result<(), String> {
        for slot in 0..self.vehicle_tracks.allocated {
            let key = format!("engine.vehicle.track.{slot}");
            if self.scene.runtime_entity_state(&key).is_some() {
                self.scene.remove_runtime_entity(&key)?;
            }
        }
        self.vehicle_tracks = VehicleTracks::default();
        Ok(())
    }

    pub(super) fn vehicle_tracks_state(&self) -> Value {
        json!({
            "count": self.vehicle_tracks.expires.len(),
            "emitted": self.vehicle_tracks.emitted,
            "rim_marks_emitted": self.vehicle_tracks.rim_marks_emitted,
            "active_wheels": self.vehicle_tracks.anchors.len()
        })
    }

    pub(super) fn sync_vehicle_tracks(&mut self) -> Result<(), String> {
        let config = self
            .settings
            .variables
            .get("engine_vehicle_tracks")
            .cloned()
            .unwrap_or(Value::Null);
        if config.get("enabled").and_then(Value::as_bool) != Some(true) {
            if !self.vehicle_tracks.anchors.is_empty() || !self.vehicle_tracks.expires.is_empty() {
                self.clear_vehicle_tracks()?;
            }
            return Ok(());
        }
        let Some(model) = config.get("model").and_then(Value::as_str) else {
            return Ok(());
        };
        let number = |key: &str, default: f32| {
            config
                .get(key)
                .and_then(Value::as_f64)
                .filter(|v| v.is_finite())
                .map_or(default, |v| v as f32)
        };
        let capacity = config
            .get("capacity")
            .and_then(Value::as_u64)
            .unwrap_or(512)
            .clamp(16, 2048) as usize;
        let step = number("step", 0.3).clamp(0.05, 1.0);
        let lifetime = number("lifetime_seconds", 90.0).clamp(1.0, 600.0);
        let gap = number("max_gap", 2.0).clamp(step, 5.0);
        // Telemetry normalizes longitudinal and lateral slip against the
        // tyre's authored peak. Legacy thresholds remain readable for old maps.
        let _legacy_slip_long = number("longitudinal_slip", 0.18).max(0.01);
        let _legacy_slip_lat = number("lateral_slip", 0.14).max(0.01);
        let slip_start = number("slip_intensity", 0.92).clamp(0.35, 4.0);
        let slip_release = (slip_start * number("release_ratio", 0.68).clamp(0.35, 0.95)).max(0.2);
        let expired = self
            .vehicle_tracks
            .expires
            .iter()
            .filter(|(slot, end)| **end <= self.elapsed_seconds || **slot >= capacity)
            .map(|(slot, _)| *slot)
            .collect::<Vec<_>>();
        for slot in expired {
            self.scene.set_runtime_entity_transform(
                &format!("engine.vehicle.track.{slot}"),
                Some([0.0, -10000.0, 0.0]),
                None,
                None,
            )?;
            self.vehicle_tracks.expires.remove(&slot);
        }
        let entities = self.vehicles.entity_ids().collect::<Vec<_>>();
        let mut alive = BTreeSet::new();
        for entity in entities {
            let Some(telemetry) = self.vehicles.telemetry(entity) else {
                continue;
            };
            let definition = self
                .vehicles
                .definition(entity)
                .expect("registered vehicle");
            if telemetry.speed_mps < 0.7 {
                continue;
            }
            for (index, wheel) in telemetry.wheels.iter().enumerate() {
                if !wheel.contact
                    || wheel.normal_force < 1.0
                    || matches!(
                        wheel.surface_class,
                        newviso_vehicle::VehicleSurfaceClass::Water
                            | newviso_vehicle::VehicleSurfaceClass::Ice
                            | newviso_vehicle::VehicleSurfaceClass::Snow
                    )
                {
                    continue;
                }
                if wheel.tire_condition == newviso_vehicle::TireCondition::Missing {
                    continue;
                }
                let kind = if wheel.tire_condition == newviso_vehicle::TireCondition::Rim {
                    ContactTrackKind::Rim
                } else {
                    ContactTrackKind::Tyre
                };
                let key = (entity, index);
                let was_drawing = self
                    .vehicle_tracks
                    .anchors
                    .get(&key)
                    .is_some_and(|anchor| anchor.kind == kind);
                let threshold = if kind == ContactTrackKind::Rim {
                    // A bare metal rim scrapes even with modest tyre-model slip.
                    // Straight rolling above walking speed still leaves a light
                    // metal contact trace; lateral/longitudinal slip strengthens it.
                    if was_drawing { 0.08 } else { 0.12 }
                } else if was_drawing {
                    slip_release
                } else {
                    slip_start
                };
                if wheel.slip_intensity < threshold
                    && !(kind == ContactTrackKind::Rim && telemetry.speed_mps > 2.0)
                {
                    continue;
                }
                let (Some(point), Some(normal)) =
                    (wheel.contact_position, wheel.contact_normal.and_then(unit))
                else {
                    continue;
                };
                if normal[1] < 0.25 {
                    continue;
                }
                alive.insert(key);
                let anchor = Anchor {
                    point,
                    normal,
                    surface: wheel.surface_entity,
                    material: wheel.surface_id,
                    intensity: if kind == ContactTrackKind::Rim {
                        wheel
                            .slip_intensity
                            .max((telemetry.speed_mps / 24.0).clamp(0.15, 1.0))
                    } else {
                        wheel.slip_intensity
                    },
                    kind,
                };
                let Some(previous) = self.vehicle_tracks.anchors.get(&key).copied() else {
                    self.vehicle_tracks.anchors.insert(key, anchor);
                    continue;
                };
                let distance = dot(
                    std::array::from_fn(|i| point[i] - previous.point[i]),
                    std::array::from_fn(|i| point[i] - previous.point[i]),
                )
                .sqrt();
                let intensity = ((previous.intensity + anchor.intensity) * 0.5).clamp(0.0, 3.0);
                let adaptive_step =
                    (step * (1.08 - intensity.min(2.0) * 0.18)).clamp(step * 0.58, step);
                if distance < adaptive_step {
                    continue;
                }
                self.vehicle_tracks.anchors.insert(key, anchor);
                if distance > gap {
                    continue;
                }

                // Subdivide long movement rather than stretching one rectangle
                // across a corner. This reduces faceting in fast drifts.
                let segments = (distance / adaptive_step).ceil().clamp(1.0, 8.0) as usize;
                let mut from = previous;
                for segment in 1..=segments {
                    let to = lerp_anchor(previous, anchor, segment as f32 / segments as f32);
                    let Some((position, rotation, length)) = ribbon_pose(from, to, gap) else {
                        from = to;
                        continue;
                    };
                    let base_width = definition.wheels[index].width;
                    if kind == ContactTrackKind::Rim {
                        // Never reuse the tread ribbon for bare rims. Dense,
                        // narrow surface marks form a metal scrape trace with no
                        // tyre texture/pattern.
                        let mark_spacing = (base_width * 0.18).clamp(0.018, 0.055);
                        let marks = (length / mark_spacing).ceil().clamp(1.0, 16.0) as usize;
                        let radius = (base_width * 0.10).clamp(0.012, 0.04);
                        let strength =
                            ((from.intensity + to.intensity) * 0.5).clamp(0.0, 2.5);
                        for mark_index in 0..marks {
                            let t = (mark_index as f32 + 0.5) / marks as f32;
                            let sample = lerp_anchor(from, to, t);
                            self.scene.add_surface_mark(SceneSurfaceMark {
                                position: std::array::from_fn(|axis| {
                                    sample.point[axis] + sample.normal[axis] * 0.006
                                }),
                                normal: sample.normal,
                                radius,
                                color: [
                                    0.10,
                                    0.10,
                                    0.10,
                                    (0.28 + strength * 0.18).clamp(0.28, 0.72),
                                ],
                            })?;
                            self.vehicle_tracks.rim_marks_emitted += 1;
                        }
                    } else {
                        let mark_strength =
                            ((from.intensity + to.intensity) * 0.5).clamp(slip_release, 2.5);
                        let width = base_width
                            * (0.92
                                + (mark_strength - slip_release).max(0.0) * 0.07)
                                .clamp(0.9, 1.18);
                        let slot = self.vehicle_tracks.next % capacity;
                        self.vehicle_tracks.next = (slot + 1) % capacity;
                        self.vehicle_tracks.allocated =
                            self.vehicle_tracks.allocated.max(slot + 1);
                        self.scene.upsert_runtime_dynamic_entity(
                            &format!("engine.vehicle.track.{slot}"),
                            SceneRuntimeEntityDesc {
                                asset_ref: Some(model.to_owned()),
                                position,
                                rotation_degrees: rotation,
                                scale: [width, 1.0, length + 0.012],
                                bounds_half_extent: [
                                    width * 0.5,
                                    0.02,
                                    (length + 0.012) * 0.5,
                                ],
                                solid: false,
                                visible_distance: 220.0,
                                stream_distance: 260.0,
                                fade_range: 28.0,
                                ..Default::default()
                            },
                        )?;
                        self.vehicle_tracks
                            .expires
                            .insert(slot, self.elapsed_seconds + f64::from(lifetime));
                    }
                    self.vehicle_tracks.emitted += 1;
                    from = to;
                }
            }
        }
        self.vehicle_tracks
            .anchors
            .retain(|key, _| alive.contains(key));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn anchor(point: [f32; 3]) -> Anchor {
        Anchor {
            point,
            normal: [0.0, 1.0, 0.0],
            surface: Some(1),
            material: Some(2),
            intensity: 1.0,
            kind: ContactTrackKind::Tyre,
        }
    }
    #[test]
    fn ribbons_break_at_teleports_and_surface_changes() {
        let a = anchor([0.0; 3]);
        let mut b = anchor([0.0, 0.0, 1.0]);
        assert!(ribbon_pose(a, b, 2.0).is_some());
        b.surface = Some(3);
        assert!(ribbon_pose(a, b, 2.0).is_none());
        b = anchor([0.0, 0.0, 1.0]);
        b.kind = ContactTrackKind::Rim;
        assert!(ribbon_pose(a, b, 2.0).is_none());
        assert!(ribbon_pose(a, anchor([0.0, 0.0, 10.0]), 2.0).is_none());
    }
    #[test]
    fn interpolated_track_anchor_preserves_surface_and_normalizes_normal() {
        let a = anchor([0.0, 0.0, 0.0]);
        let mut b = anchor([1.0, 0.0, 1.0]);
        b.normal = unit([0.25, 1.0, 0.0]).unwrap();
        b.intensity = 2.0;
        let mid = lerp_anchor(a, b, 0.5);
        assert!((mid.point[0] - 0.5).abs() < 1e-6);
        assert!((mid.point[2] - 0.5).abs() < 1e-6);
        assert!((dot(mid.normal, mid.normal) - 1.0).abs() < 1e-5);
        assert!((mid.intensity - 1.5).abs() < 1e-6);
    }

    #[test]
    fn ribbon_follows_banked_surface_and_has_no_height_thickness() {
        let mut a = anchor([0.0; 3]);
        a.normal = unit([0.4, 1.0, 0.0]).unwrap();
        let mut b = a;
        b.point = [0.0, 0.0, 0.4];
        let (p, r, l) = ribbon_pose(a, b, 2.0).unwrap();
        assert!(p[0] > 0.0);
        assert!(r[2].abs() > 10.0);
        assert!((l - 0.4).abs() < 1e-6);
    }
}
