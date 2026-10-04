use super::*;

impl EngineApplication {
    pub(super) fn apply_vehicles_command(
        &mut self,
        command: &Value,
        index: usize,
        op: &str,
    ) -> Result<(), String> {
        let resolved = match op {
            "vehicle.upsert"
            | "vehicle.remove"
            | "vehicle.input.set"
            | "vehicle.enabled.set"
            | "vehicle.health.set"
            | "vehicle.specification.set"
            | "vehicle.damage_policy.set"
            | "vehicle.damage.apply"
            | "vehicle.alarm.set"
            | "vehicle.thermal.set"
            | "vehicle.explode"
            | "vehicle.fuel.set"
            | "vehicle.tire.set"
            | "vehicle.part.set"
            | "vehicle.cabin.set"
            | "vehicle.lights.set"
            | "vehicle.audio_fx.configure"
            | "vehicle.occupant.set"
            | "vehicle.access.layout.set"
            | "vehicle.access.reserve"
            | "vehicle.access.release" => {
                self.resolve_script_scene_entity_command(command, index, op)?
                    .0
            }
            _ => std::borrow::Cow::Borrowed(command),
        };
        let command = resolved.as_ref();
        match op {
            "vehicle.surface_policy.set" => {
                self.set_vehicle_surface_policy_from_script(command, index)?;
            }
            "vehicle.upsert" => {
                self.upsert_vehicle_from_script(command, index)?;
            }
            "vehicle.remove" => {
                self.remove_vehicle_from_script(command, index)?;
            }
            "vehicle.input.set" => {
                self.set_vehicle_input_from_script(command, index)?;
            }
            "vehicle.enabled.set" => {
                self.set_vehicle_enabled_from_script(command, index)?;
            }
            "vehicle.health.set" => {
                self.set_vehicle_health_from_script(command, index)?;
            }
            "vehicle.specification.set" => {
                self.set_vehicle_specification_from_script(command, index)?;
            }
            "vehicle.damage_policy.set" => {
                self.set_vehicle_damage_policy_from_script(command, index)?;
            }
            "vehicle.damage.apply" => {
                self.apply_vehicle_damage_from_script(command, index)?;
            }
            "vehicle.alarm.set" => {
                self.set_vehicle_alarm_from_script(command, index)?;
            }
            "vehicle.thermal.set" => {
                self.set_vehicle_thermal_from_script(command, index)?;
            }
            "vehicle.explode" => {
                self.explode_vehicle_from_script(command, index)?;
            }
            "vehicle.environment.set" => {
                let temperature = command_number(command, "ambient_temperature_celsius", index)?;
                self.vehicles.set_ambient_temperature(temperature)?;
            }
            "vehicle.fuel_policy.set" => {
                self.set_vehicle_fuel_policy_from_script(command, index)?;
            }
            "vehicle.fuel.set" => {
                self.set_vehicle_fuel_from_script(command, index)?;
            }
            "vehicle.tire.set" => {
                self.set_vehicle_tire_from_script(command, index)?;
            }
            "vehicle.part.set" => {
                self.set_vehicle_part_from_script(command, index)?;
            }
            "vehicle.tracks.clear" => {
                self.clear_vehicle_tracks()?;
            }
            "vehicle.cabin.set" => {
                self.set_vehicle_cabin_from_script(command, index)?;
            }
            "vehicle.lights.set" => {
                self.set_vehicle_lights_from_script(command, index)?;
            }
            "vehicle.audio_fx.configure" => {
                self.set_vehicle_audio_fx_from_script(command, index)?;
            }
            "vehicle.occupant.set" => {
                self.set_vehicle_occupant_from_script(command, index)?;
            }
            "vehicle.access.layout.set" => {
                self.set_vehicle_access_layout_from_script(command, index)?;
            }
            "vehicle.access.reserve" => {
                self.reserve_vehicle_access_from_script(command, index)?;
            }
            "vehicle.access.release" => {
                self.release_vehicle_access_from_script(command, index)?;
            }
            _ => return Err(unsupported_command(op, index)),
        }
        Ok(())
    }
}
