use super::*;
use newviso_scene::SceneVehicleDashboard;

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct VehicleDashboardState {
    pub display: SceneVehicleDashboard,
    pub last_update_seconds: Option<f64>,
}

impl EngineApplication {
    pub(super) fn sync_vehicle_dashboard(
        &mut self,
        entity: u64,
        telemetry: &newviso_vehicle::VehicleTelemetry,
    ) {
        let Some(binding) = self.vehicle_presentations.get_mut(&entity) else {
            return;
        };
        let state = &mut binding.dashboard;
        let dt = state
            .last_update_seconds
            .map_or(0.0, |t| (self.elapsed_seconds - t).clamp(0.0, 0.1) as f32);
        state.last_update_seconds = Some(self.elapsed_seconds);
        let blend = 1.0 - (-12.0 * dt).exp();
        let running = telemetry.engine_running && !telemetry.exploded;
        let revs = if running {
            // CalculateDialRPMRatio maps the normalized engine playback range
            // so idle occupies 10% of the authored tachometer scale.
            0.1 + 0.9 * telemetry.engine_speed.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let revs = if telemetry.gear < 0 && revs > 0.1 {
            0.1 + (revs - 0.1) * 0.25
        } else {
            revs
        };
        let speed = telemetry
            .wheels
            .iter()
            .filter(|w| w.tire_condition != TireCondition::Missing)
            .map(|w| (w.angular_velocity * w.effective_radius).abs())
            .fold(0.0f32, f32::max);
        // Wheel rotation drives the original speedometer, including wheelspin.
        let speed = if telemetry.wheels.iter().any(|w| w.contact) {
            speed
        } else {
            telemetry.speed_forward_mps.abs()
        };
        state.display.speed_mph += (speed * 2.236936 - state.display.speed_mph) * blend;
        state.display.revs += (revs - state.display.revs) * blend;
        // A stopped tachometer must return to zero even after a short frame.
        if !running && state.display.revs < 0.001 {
            state.display.revs = 0.0;
        }
        state.display.fuel = telemetry.fuel_fraction.clamp(0.0, 1.0);
        let mut temperature = if running {
            (telemetry.thermal.temperature_celsius / 120.0).clamp(0.0, 1.0)
        } else {
            0.0
        };
        if temperature > 0.4
            && temperature < 0.9
            && (binding.engine_health > 400.0 || temperature < 0.5)
        {
            temperature = 0.5;
        }
        state.display.engine_temperature +=
            (temperature - state.display.engine_temperature).clamp(-dt * 0.1, dt * 0.1);
        state.display.oil_temperature = (telemetry.thermal.temperature_celsius / 120.0)
            .clamp(0.0, 1.0)
            .powi(3);
        let oil = if telemetry.oil_capacity > 0.0 {
            telemetry.oil_level / telemetry.oil_capacity
        } else {
            1.0
        };
        let pressure = if running
            && !telemetry.engine_starting
            && oil >= 0.5
            && telemetry.engine_fire_level <= 0.0
        {
            (state.display.revs + 1.0 - state.display.oil_temperature).clamp(0.0, 0.6)
        } else {
            0.0
        };
        state.display.oil_pressure +=
            (pressure - state.display.oil_pressure).clamp(-dt * 0.1, dt * 0.1);
        state.display.vacuum = if running {
            (1.0 - telemetry.input.throttle.abs()).clamp(0.0, 1.0)
        } else {
            0.0
        };
        state.display.boost = 0.0; // Both current profiles are naturally aspirated.
        state.display.gear = telemetry.gear;
        state.display.odometer_miles += telemetry.speed_mps.max(0.0) * dt / 1609.344;
        let blink = ((self.elapsed_seconds * 1.8).floor() as i64 & 1) == 0;
        let left = (binding.lights.left_indicator || binding.lights.hazard) && blink;
        let right = (binding.lights.right_indicator || binding.lights.hazard) && blink;
        let conditions = [
            left,
            right,
            telemetry.input.handbrake > 0.1,
            telemetry.engine_fire_level > 0.0 || telemetry.engine_condition < 0.4,
            telemetry.engine_starting,
            telemetry.fuel_fraction < 0.2,
            oil < 0.5 || telemetry.engine_starting,
            binding.lights.headlights,
            binding.cabin.high_beam && binding.lights.headlights,
            telemetry.engine_starting,
            running || telemetry.engine_starting,
            binding.lights.headlights,
        ];
        state.display.lamps = conditions
            .iter()
            .enumerate()
            .fold(0, |bits, (i, &on)| bits | ((on as u32) << i));
        self.scene.set_vehicle_dashboard(entity, state.display);
    }
}
