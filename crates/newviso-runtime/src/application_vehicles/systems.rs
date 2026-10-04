use super::*;
use newviso_vehicle::{VehicleDamagePolicy, VehicleHealthUpdate, VehicleThermalState};

fn vehicle_entity(command: &Value) -> Result<u64, String> {
    command
        .get("entity")
        .and_then(Value::as_u64)
        .ok_or_else(|| "vehicle command requires resolved entity".into())
}

impl EngineApplication {
    pub(crate) fn set_vehicle_health_from_script(
        &mut self,
        command: &Value,
        _index: usize,
    ) -> Result<(), String> {
        let entity = vehicle_entity(command)?;
        let update = serde_json::from_value::<VehicleHealthUpdate>(
            command
                .get("health")
                .cloned()
                .ok_or("vehicle.health.set requires health object")?,
        )
        .map_err(|e| e.to_string())?;
        self.vehicles.set_health(entity, update)?;
        if let (Some(state), Some(binding)) = (
            self.vehicles.damage_state(entity),
            self.vehicle_presentations.get_mut(&entity),
        ) {
            binding.body_health = state.body_health;
            binding.engine_health = state.engine_health;
        }
        Ok(())
    }

    pub(crate) fn set_vehicle_damage_policy_from_script(
        &mut self,
        command: &Value,
        _index: usize,
    ) -> Result<(), String> {
        let policy = serde_json::from_value::<VehicleDamagePolicy>(
            command
                .get("policy")
                .cloned()
                .ok_or("vehicle.damage_policy.set requires policy object")?,
        )
        .map_err(|e| e.to_string())?;
        self.vehicles
            .set_damage_policy(vehicle_entity(command)?, policy)
    }

    pub(crate) fn set_vehicle_alarm_from_script(
        &mut self,
        command: &Value,
        _index: usize,
    ) -> Result<(), String> {
        let entity = vehicle_entity(command)?;
        let armed = command
            .get("armed")
            .map(|v| v.as_bool().ok_or("armed must be boolean"))
            .transpose()?;
        let trigger = command
            .get("trigger")
            .map(|v| v.as_bool().ok_or("trigger must be boolean"))
            .transpose()?;
        if armed.is_none() && trigger.is_none() {
            return Err("vehicle.alarm.set requires armed or trigger".into());
        }
        if let Some(value) = armed {
            self.vehicles.arm_alarm(entity, value)?;
        }
        if trigger == Some(true) {
            self.vehicles.trigger_alarm(entity)?;
        }
        Ok(())
    }

    pub(crate) fn set_vehicle_thermal_from_script(
        &mut self,
        command: &Value,
        _index: usize,
    ) -> Result<(), String> {
        let entity = vehicle_entity(command)?;
        let state = serde_json::from_value::<VehicleThermalState>(
            command
                .get("state")
                .cloned()
                .ok_or("vehicle.thermal.set requires state object")?,
        )
        .map_err(|e| e.to_string())?;
        self.vehicles.set_thermal_state(entity, state)
    }

    pub(crate) fn explode_vehicle_from_script(
        &mut self,
        command: &Value,
        _index: usize,
    ) -> Result<(), String> {
        self.vehicles.explode(vehicle_entity(command)?)?;
        Ok(())
    }

    pub(crate) fn apply_vehicle_damage_from_script(
        &mut self,
        command: &Value,
        _index: usize,
    ) -> Result<(), String> {
        let entity = vehicle_entity(command)?;
        let request = serde_json::from_value::<VehicleDamageRequest>(
            command
                .get("request")
                .cloned()
                .ok_or("vehicle.damage.apply requires request object")?,
        )
        .map_err(|e| e.to_string())?;
        if !request.raw_damage.is_finite() || request.raw_damage < 0.0 {
            return Err("damage must be finite and non-negative".into());
        }
        let transform = self
            .scene
            .entity_transform_values(entity)
            .ok_or("vehicle transform is unavailable")?;
        let kind = match request.damage_type {
            VehicleDamageType::Collision => application_physics::PhysicsDamageKind::Collision,
            VehicleDamageType::Bullet => application_physics::PhysicsDamageKind::Bullet,
            VehicleDamageType::Explosive => application_physics::PhysicsDamageKind::Explosive,
            VehicleDamageType::Fire => application_physics::PhysicsDamageKind::Fire,
            VehicleDamageType::Melee => application_physics::PhysicsDamageKind::Melee,
            VehicleDamageType::Water => application_physics::PhysicsDamageKind::Water,
            VehicleDamageType::Script => application_physics::PhysicsDamageKind::Script,
        };
        self.apply_vehicle_damage_transaction_to_part(
            application_physics::PhysicsDamageContact {
                target: entity,
                source: request.source_entity.unwrap_or(0),
                damage_kind: kind,
                direct_damage: request.raw_damage,
                contact_impulse: request.contact_impulse,
                point: vehicle_local_point(
                    transform.0,
                    transform.1,
                    transform.2,
                    request.local_position,
                ),
                impulse_direction: rotate_euler_xyz(request.local_direction, transform.1),
            },
            (request.component != VehicleDamageComponent::Unknown).then_some(request.component),
            request.part_index,
        )?;
        Ok(())
    }
}
