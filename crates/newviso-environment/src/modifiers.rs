use super::{TimecycleFrame, TimecycleModifier};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModifierMode {
    None,
    Interp,
    InterpMult,
    Clamp,
    Max,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct ModifierOperation {
    pub mode: ModifierMode,
    pub a: f32,
    pub b: f32,
}
impl ModifierOperation {
    pub fn apply(self, base: f32, strength: f32) -> f32 {
        match self.mode {
            ModifierMode::None => base,
            ModifierMode::Interp => (1.0 - strength) * base + strength * self.a,
            ModifierMode::InterpMult => {
                let t = self.b * strength;
                (1.0 - t) * base + t * self.a
            }
            ModifierMode::Clamp => {
                let lower = if base < self.a {
                    (1.0 - strength) * base + strength * self.a
                } else {
                    base
                };
                if base > self.b {
                    (1.0 - strength) * base + strength * self.b
                } else {
                    lower
                }
            }
            ModifierMode::Max => ((1.0 - strength) * base).max(strength * self.a),
        }
    }
}
#[derive(Clone, Debug, Deserialize)]
pub struct TimecycleModifierCatalog {
    pub defaults: BTreeMap<String, f32>,
    pub modifiers: BTreeMap<String, TimecycleModifier>,
}
impl TimecycleModifierCatalog {
    pub fn validate(&self) -> Result<(), String> {
        if self.defaults.is_empty() || self.defaults.values().any(|v| !v.is_finite()) {
            return Err("invalid timecycle defaults".into());
        }
        let mut frame = TimecycleFrame::default();
        for m in self.modifiers.values() {
            frame.apply_modifier(m)?;
        }
        Ok(())
    }
    pub fn fill_defaults(&self, frame: &mut TimecycleFrame) {
        for (key, value) in &self.defaults {
            frame.channels.entry(key.clone()).or_insert(*value);
        }
    }
    pub fn modifier(&self, name: &str, strength: f32) -> Result<TimecycleModifier, String> {
        if !strength.is_finite() || !(0.0..=1.0).contains(&strength) {
            return Err("invalid modifier strength".into());
        }
        let mut modifier = self
            .modifiers
            .get(&name.to_ascii_lowercase())
            .ok_or_else(|| format!("unknown timecycle modifier '{name}'"))?
            .clone();
        modifier.strength = strength;
        Ok(modifier)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn modifier_modes_match_reference_val_a_val_b_rules() {
        let op = |mode, a, b| ModifierOperation { mode, a, b }.apply(10.0, 0.5);
        assert_eq!(op(ModifierMode::None, 2.0, 0.0), 10.0);
        assert_eq!(op(ModifierMode::Interp, 2.0, 0.0), 6.0);
        assert_eq!(op(ModifierMode::InterpMult, 2.0, 0.25), 9.0);
        assert_eq!(op(ModifierMode::Clamp, 1.0, 4.0), 7.0);
        assert_eq!(op(ModifierMode::Clamp, 12.0, 20.0), 11.0);
        assert_eq!(op(ModifierMode::Max, 20.0, 0.0), 10.0);
    }
}
