use super::*;

impl EngineApplication {
    pub(super) fn sync_physical_particle_bodies(&mut self) -> Result<(), String> {
        let Some(physics) = self.physics.as_mut() else {
            return Ok(());
        };
        for entity in self.scene.drain_removed_physical_particles() {
            if physics.vehicle_body_state(entity).is_some() {
                physics.destroy_body_from_script(&json!({"entity":entity}), 0)?;
            }
        }
        for spawn in self.scene.drain_physical_particle_spawns() {
            let half = spawn.half_extents;
            let hull = application_vehicles::particle_fragment_collision_hull(spawn.hull);
            physics.upsert_body_from_script(&json!({
                "entity":spawn.entity,"body_kind":"dynamic",
                "shape":{"kind":"box","half_extents":half},
                "position":spawn.position,"rotation":[0.0,0.0,0.0,1.0],
                "linear_velocity":spawn.velocity,"angular_velocity":spawn.angular_velocity,
                "density":spawn.mass/(8.0*half[0]*half[1]*half[2]).max(0.000001),
                "mass_properties":{"mass":spawn.mass,"center_of_mass":[0.0,0.0,0.0],
                    "inertia_diagonal":std::array::from_fn::<_,3,_>(|i|spawn.mass/3.0*(half[(i+1)%3].powi(2)+half[(i+2)%3].powi(2)).max(0.000001))},
                "convex_hulls":if hull.len()>=4 {vec![hull]} else {Vec::new()},
                "friction":0.65,"restitution":spawn.restitution,
                "linear_damping":0.04,"angular_damping":0.12,
                "participates_in_queries":true,"casts_contacts":true,"continuous_collision":true
            }),0)?;
            self.scene.set_physics_process_active(spawn.entity, true)?;
        }
        Ok(())
    }
}
