//! Source-format independent calendar, weather transitions and timecycle evaluation.
//! All authored samples, weather profiles and tuning come from mounted assets.
mod celestial;
mod clock;
mod cloud_selection;
mod clouds;
mod modifiers;
mod timecycle;
mod weather;
pub use celestial::{CelestialFrame, CelestialPolicy};
pub use clock::{CalendarDate, GameClock};
pub use cloud_selection::{CloudHatList, CloudHatSelection};
pub use clouds::CloudKeyframeCatalog;
pub use modifiers::{ModifierMode, ModifierOperation, TimecycleModifierCatalog};
pub use timecycle::{TimeSample, TimecycleCatalog, TimecycleFrame, TimecycleModifier};
pub use weather::{WeatherCycleEntry, WeatherMachine, WeatherSimulation, WeatherTuning};

pub(crate) fn approach(current: f32, target: f32, step: f32) -> f32 {
    current + (target - current).clamp(-step.max(0.0), step.max(0.0))
}
pub(crate) fn valid_dt(dt: f64) -> Result<(), String> {
    if !dt.is_finite() || dt < 0.0 || dt > 1.0e9 {
        Err("environment delta must be finite and in 0..=1e9 seconds".into())
    } else {
        Ok(())
    }
}

pub mod snow;
pub mod surface_snow;
pub mod thermal;
