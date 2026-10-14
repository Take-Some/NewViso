use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CelestialPolicy {
    pub sunrise: f32,
    pub sunset: f32,
    pub sun_roll_degrees: f32,
    pub sun_yaw_degrees: f32,
    pub moon_roll_offset_degrees: f32,
    pub moon_wobble_frequency: f32,
    pub moon_wobble_amplitude: f32,
    pub moon_wobble_offset: f32,
    pub lunar_period_days: f64,
    pub lunar_orbit_period_days: f64,
    pub moon_dim_multiplier: f32,
    pub direction_min_height: f32,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct CelestialFrame {
    pub sun_direction: [f32; 3],
    pub moon_direction: [f32; 3],
    pub light_direction: [f32; 3],
    pub sun_fade: f32,
    pub moon_fade: f32,
    pub light_intensity: f32,
    pub lunar_phase: f32,
}
fn norm(v: [f32; 3]) -> [f32; 3] {
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-8);
    v.map(|x| x / n)
}
fn from_z_up(v: [f32; 3]) -> [f32; 3] {
    [v[0], v[2], -v[1]]
}
impl CelestialPolicy {
    pub fn validate(&self) -> Result<(), String> {
        let vals = [
            self.sunrise,
            self.sunset,
            self.sun_roll_degrees,
            self.sun_yaw_degrees,
            self.moon_roll_offset_degrees,
            self.moon_wobble_frequency,
            self.moon_wobble_amplitude,
            self.moon_wobble_offset,
            self.moon_dim_multiplier,
            self.direction_min_height,
        ];
        if vals.iter().any(|x| !x.is_finite())
            || self.sunrise <= 0.0
            || self.sunset <= self.sunrise
            || self.sunset >= 24.0
            || !self.lunar_period_days.is_finite()
            || self.lunar_period_days <= 1.0
            || !self.lunar_orbit_period_days.is_finite()
            || self.lunar_orbit_period_days <= 1.0
            || !(0.0..=1.0).contains(&self.moon_dim_multiplier)
            || !(0.0..=1.0).contains(&self.direction_min_height)
        {
            return Err("invalid celestial policy".into());
        }
        Ok(())
    }
    /// Reproduces the piecewise solar orbit and independently moving lunar orbit.
    pub fn evaluate(&self, hours: f64, days: f64) -> CelestialFrame {
        let pi = std::f32::consts::PI;
        let h = hours.rem_euclid(24.0) as f32;
        let a = if h < self.sunrise {
            0.5 * pi * h / self.sunrise
        } else if h < self.sunset {
            0.5 * pi + pi * (h - self.sunrise) / (self.sunset - self.sunrise)
        } else {
            1.5 * pi + 0.5 * pi * (h - self.sunset) / (24.0 - self.sunset)
        };
        let slope = self.sun_roll_degrees.to_radians();
        let yaw = self.sun_yaw_degrees.to_radians();
        let raw = [a.sin(), -a.cos() * slope.cos(), -a.cos() * slope.sin()];
        let sun = norm([
            raw[0] * yaw.cos() - raw[1] * yaw.sin(),
            raw[0] * yaw.sin() + raw[1] * yaw.cos(),
            raw[2],
        ]);
        let lunar_day = days.rem_euclid(self.lunar_period_days);
        let half = (self.lunar_orbit_period_days - 1.0) * 0.5;
        let ma = a + pi * ((days.rem_euclid(self.lunar_orbit_period_days) - half) / half) as f32;
        let ms = (self.sun_roll_degrees + self.moon_roll_offset_degrees).to_radians();
        let wobble = self.moon_wobble_offset
            + (lunar_day as f32 * 2.0 * pi * self.moon_wobble_frequency).sin()
                * self.moon_wobble_amplitude;
        let moon = norm([
            ma.sin(),
            -ma.cos() * ms.cos() - ms.sin() * wobble,
            -ma.cos() * ms.sin() + ms.cos() * wobble,
        ]);
        let (sun_fade, mut moon_fade) = if h > self.sunset {
            (
                1.0 - ((h - self.sunset - 1.5) * 4.0).clamp(0.0, 1.0),
                ((h - self.sunset - 1.75) * 4.0).clamp(0.0, 1.0),
            )
        } else if h < self.sunrise {
            (
                ((h - self.sunrise + 1.25) * 4.0).clamp(0.0, 1.0),
                1.0 - ((h - self.sunrise + 1.5) * 4.0).clamp(0.0, 1.0),
            )
        } else {
            (1.0, 0.0)
        };
        moon_fade *= (moon[2] / 0.1).clamp(0.0, 1.0);
        let total = sun_fade * 100.0 + moon_fade;
        let mut light = if total > 1e-8 {
            std::array::from_fn(|i| (sun[i] * sun_fade * 100.0 + moon[i] * moon_fade) / total)
        } else {
            [0.0, 0.0, 1.0]
        };
        light[2] = light[2].max(self.direction_min_height);
        light = norm(light);
        let phase = (lunar_day / (self.lunar_period_days - 1.0)) as f32;
        let dim = (phase - 0.5).abs() * 2.0 * moon_fade;
        let intensity = sun_fade.max(moon_fade) * (1.0 - dim * (1.0 - self.moon_dim_multiplier));
        CelestialFrame {
            sun_direction: from_z_up(sun),
            moon_direction: from_z_up(moon),
            light_direction: from_z_up(light),
            sun_fade,
            moon_fade,
            light_intensity: intensity,
            lunar_phase: phase,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn p() -> CelestialPolicy {
        CelestialPolicy {
            sunrise: 6.0,
            sunset: 20.0,
            sun_roll_degrees: 122.0,
            sun_yaw_degrees: 0.0,
            moon_roll_offset_degrees: -122.0,
            moon_wobble_frequency: 2.0,
            moon_wobble_amplitude: 0.2,
            moon_wobble_offset: 0.375,
            lunar_period_days: 27.0,
            lunar_orbit_period_days: 55.0,
            moon_dim_multiplier: 0.3,
            direction_min_height: 0.33,
        }
    }
    #[test]
    fn horizon_and_orbits() {
        let p = p();
        assert!(p.evaluate(6.0, 0.0).sun_direction[1].abs() < 1e-6);
        assert!(p.evaluate(20.0, 0.0).sun_direction[1].abs() < 1e-6);
        assert!(p.evaluate(13.0, 0.0).sun_direction[1] > 0.8);
        assert_ne!(
            p.evaluate(0.0, 0.0).moon_direction,
            p.evaluate(0.0, 7.0).moon_direction
        );
    }
}
