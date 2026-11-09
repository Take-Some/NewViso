//! Observe each provider fixed output once, before frame coalescing can hide an impact.
use super::*;
#[derive(Debug, Default)]
pub(super) struct DoorContactWork {
    pub load_nm: f32,
    pub energy: f32,
    peak: Option<Impact>,
}
#[derive(Debug)]
struct Impact {
    energy: f32,
    point: [f32; 3],
    direction: [f32; 3],
    lever: f32,
    momentum: f32,
    outgoing: f32,
}
impl PhysicsRuntime {
    pub(crate) fn observe_door_contacts(&mut self, dt: f32) {
        for event in &self.last_output.events {
            let (c, begin) = match event {
                PhysicsEvent::ContactBegin(c) => (c, true),
                PhysicsEvent::ContactPersist(c) => (c, false),
                _ => continue,
            };
            for id in [c.a, c.b] {
                let Some(b) = self.doors.entries.get_mut(&id) else {
                    continue;
                };
                let Some(g) = &b.geometry else { continue };
                let axis = dm::rotate(g.rotation, dm::rotate(g.frame, [0.0, 1.0, 0.0]));
                let arm = dm::sub(c.point, g.hinge);
                let n = c.normal;
                let cross = [
                    arm[1] * n[2] - arm[2] * n[1],
                    arm[2] * n[0] - arm[0] * n[2],
                    arm[0] * n[1] - arm[1] * n[0],
                ];
                let lever = dm::dot(cross, axis);
                b.contact.load_nm += lever * c.impulse / dt;
                if !begin {
                    continue;
                }
                let other = if c.a == id { c.b } else { c.a };
                let Some(p) = self
                    .bodies
                    .get(&other)
                    .filter(|p| p.kind == PhysicsBodyKind::Dynamic)
                else {
                    continue;
                };
                let mass = p.mass_properties.map_or_else(
                    || volume_and_extent(p.shape).0 * p.material.density,
                    |m| m.mass,
                );
                let speed = c.relative_velocity.map_or(0.0, |v| dm::dot(v, n).max(0.0));
                // CCD may expose the contact after its last internal solver
                // impulse has returned to zero. Preserve the measured incoming
                // momentum for a yielding catch as well as fracture work.
                b.contact.load_nm += lever * (mass * speed - c.impulse).max(0.0) / dt;
                let energy = dm::incident_energy(mass, speed, lever.abs() / g.width());
                b.contact.energy += energy;
                if energy > 0.0 && b.contact.peak.as_ref().is_none_or(|p| energy > p.energy) {
                    b.contact.peak = Some(Impact {
                        energy,
                        point: c.point,
                        direction: dm::mul(n, if c.a == id { 1.0 } else { -1.0 }),
                        lever: lever.abs(),
                        momentum: mass * speed,
                        outgoing: dm::incident_energy(
                            mass,
                            dm::dot(p.linear_velocity, n).abs(),
                            lever.abs() / g.width(),
                        ),
                    });
                }
            }
        }
    }
    pub(super) fn continue_door_impact(
        &mut self,
        b: &DoorBinding,
        contact: DoorContactWork,
        released: f32,
    ) -> Result<(), String> {
        let Some(hit) = contact.peak else {
            return Ok(());
        };
        let available = (released.min(hit.energy) - hit.outgoing).max(0.0);
        if available == 0.0 {
            return Ok(());
        }
        let g = b.geometry.as_ref().unwrap();
        let p = &self.bodies[&b.definition.entity];
        let mass = p.mass_properties.map_or_else(
            || volume_and_extent(p.shape).0 * p.material.density,
            |m| m.mass,
        );
        let h = g.half;
        let diagonal = p.mass_properties.map_or(
            [
                mass * (h[1] * h[1] + h[2] * h[2]) / 3.0,
                mass * (h[0] * h[0] + h[2] * h[2]) / 3.0,
                mass * (h[0] * h[0] + h[1] * h[1]) / 3.0,
            ],
            |m| m.inertia_diagonal,
        );
        let axis = dm::rotate(g.frame, [0.0, 1.0, 0.0]);
        let inertia = (0..3).map(|i| diagonal[i] * axis[i] * axis[i]).sum::<f32>()
            + mass * (dm::dot(g.pivot, g.pivot) - dm::dot(g.pivot, axis).powi(2));
        let impulse = dm::fracture_impulse(available, inertia, hit.lever, hit.momentum);
        if impulse > 0.0 {
            self.apply_impulse_from_script(&json!({"entity":b.definition.entity,"point":hit.point,"impulse":dm::mul(hit.direction,impulse)}),0)?;
        }
        Ok(())
    }
}
