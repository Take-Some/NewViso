use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TimeSample {
    pub name: String,
    pub hour: f32,
    pub duration: f32,
}
#[derive(Clone, Debug)]
struct Region {
    channels: BTreeMap<String, Vec<f32>>,
}
#[derive(Clone, Debug)]
pub struct TimecycleCatalog {
    pub samples: Vec<TimeSample>,
    cycles: BTreeMap<String, BTreeMap<String, Region>>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TimecycleFrame {
    pub channels: BTreeMap<String, f32>,
    pub before: usize,
    pub after: usize,
    pub time_blend: f32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TimecycleModifier {
    pub id: String,
    pub strength: f32,
    #[serde(default)]
    pub channels: BTreeMap<String, f32>,
    #[serde(default)]
    pub operations: BTreeMap<String, super::ModifierOperation>,
}
impl TimecycleFrame {
    pub fn number(&self, key: &str, fallback: f32) -> f32 {
        self.channels.get(key).copied().unwrap_or(fallback)
    }
    pub fn color(&self, prefix: &str, fallback: [f32; 3]) -> [f32; 3] {
        std::array::from_fn(|i| {
            self.number(&format!("{prefix}_{}", ["r", "g", "b"][i]), fallback[i])
        })
    }
    pub fn apply_modifier(&mut self, modifier: &TimecycleModifier) -> Result<(), String> {
        if !modifier.strength.is_finite()
            || !(0.0..=1.0).contains(&modifier.strength)
            || modifier.channels.values().any(|v| !v.is_finite())
            || modifier
                .operations
                .values()
                .any(|op| !op.a.is_finite() || !op.b.is_finite())
        {
            return Err("timecycle modifier contains invalid values".into());
        }
        for (k, v) in &modifier.channels {
            let base = self.channels.get(k).copied().unwrap_or(*v);
            self.channels
                .insert(k.clone(), base + (*v - base) * modifier.strength);
        }
        if modifier.strength > 0.0 {
            for (key, operation) in &modifier.operations {
                let base = self.channels.get(key).copied().unwrap_or(0.0);
                self.channels
                    .insert(key.clone(), operation.apply(base, modifier.strength));
            }
        }
        Ok(())
    }
}
impl TimecycleCatalog {
    pub fn parse(value: &Value, samples: Vec<TimeSample>) -> Result<Self, String> {
        if samples.len() < 2 || samples.len() > 64 {
            return Err("timecycle requires 2..=64 clock samples".into());
        }
        for (i, s) in samples.iter().enumerate() {
            let next = if i + 1 == samples.len() {
                samples[0].hour + 24.0
            } else {
                samples[i + 1].hour
            };
            if !s.hour.is_finite()
                || !(0.0..24.0).contains(&s.hour)
                || !s.duration.is_finite()
                || s.duration < 0.0
                || s.hour + s.duration >= next
            {
                return Err(format!("invalid timecycle sample '{}'", s.name));
            }
        }
        let entries = value
            .get("entries")
            .and_then(Value::as_array)
            .ok_or("timecycle catalog requires entries")?;
        let mut cycles = BTreeMap::new();
        for entry in entries {
            let cycle = entry
                .pointer("/data/cycle")
                .ok_or("timecycle entry requires data.cycle")?;
            let id = cycle
                .pointer("/_attributes/name")
                .and_then(Value::as_str)
                .ok_or("timecycle cycle requires name")?
                .to_ascii_uppercase();
            let regions = cycle
                .get("region")
                .and_then(Value::as_array)
                .ok_or("timecycle cycle requires regions")?;
            let mut parsed_regions = BTreeMap::new();
            for region in regions {
                let name = region
                    .pointer("/_attributes/name")
                    .and_then(Value::as_str)
                    .ok_or("timecycle region requires name")?
                    .to_ascii_uppercase();
                let mut channels = BTreeMap::new();
                for (key, value) in region
                    .as_object()
                    .ok_or("timecycle region must be an object")?
                {
                    if key.starts_with('_') {
                        continue;
                    }
                    let Some(text) = value.as_str() else {
                        continue;
                    };
                    let mut values = text
                        .split_whitespace()
                        .map(|v| {
                            v.parse::<f32>().map_err(|_| {
                                format!("invalid timecycle channel '{id}/{name}/{key}'")
                            })
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    if values.len() == 1 {
                        values.resize(samples.len(), values[0]);
                    }
                    if values.len() != samples.len() || values.iter().any(|v| !v.is_finite()) {
                        return Err(format!(
                            "timecycle channel '{id}/{name}/{key}' has {} samples, expected {}",
                            values.len(),
                            samples.len()
                        ));
                    }
                    channels.insert(key.clone(), values);
                }
                if channels.is_empty() {
                    return Err("timecycle region has no numeric channels".into());
                }
                if parsed_regions.insert(name, Region { channels }).is_some() {
                    return Err("duplicate timecycle region".into());
                }
            }
            if !parsed_regions.contains_key("GLOBAL") {
                return Err(format!("timecycle '{id}' requires GLOBAL region"));
            }
            if cycles.insert(id, parsed_regions).is_some() {
                return Err("duplicate timecycle cycle".into());
            }
        }
        if cycles.is_empty() {
            return Err("empty timecycle catalog".into());
        }
        Ok(Self { samples, cycles })
    }
    pub fn cycle_count(&self) -> usize {
        self.cycles.len()
    }
    pub fn contains(&self, id: &str) -> bool {
        self.cycles.contains_key(&id.to_ascii_uppercase())
    }
    pub fn sample_info(&self, hours: f64) -> (usize, usize, f32) {
        let hour = (hours.rem_euclid(24.0) as f32).min(23.999);
        let before = self
            .samples
            .iter()
            .rposition(|s| s.hour <= hour)
            .unwrap_or(self.samples.len() - 1);
        let after = (before + 1) % self.samples.len();
        let mut start = self.samples[before].hour + self.samples[before].duration;
        let mut end = self.samples[after].hour;
        if before == self.samples.len() - 1 {
            if hour < self.samples[0].hour {
                start -= 24.0;
            } else {
                end += 24.0;
            }
        }
        (
            before,
            after,
            ((hour - start) / (end - start)).clamp(0.0, 1.0),
        )
    }
    /// Temporal interpolation is performed before weather and spatial-region blending.
    /// All channels are retained; providers may consume channels not mapped by a renderer.
    pub fn evaluate(
        &self,
        hours: f64,
        weights: &BTreeMap<String, f32>,
        region: &str,
        region_blend: f32,
    ) -> Result<TimecycleFrame, String> {
        if !hours.is_finite()
            || !region_blend.is_finite()
            || !(0.0..=1.0).contains(&region_blend)
            || weights.is_empty()
            || weights.values().any(|v| !v.is_finite() || *v < 0.0)
        {
            return Err("invalid timecycle evaluation input".into());
        }
        let total: f32 = weights.values().sum();
        if total <= 0.0 || !total.is_finite() {
            return Err("empty timecycle weights".into());
        }
        let (before, after, t) = self.sample_info(hours);
        let mut frame = TimecycleFrame {
            before,
            after,
            time_blend: t,
            ..Default::default()
        };
        for (id, weight) in weights {
            if *weight <= 0.0 {
                continue;
            }
            let regions = self
                .cycles
                .get(&id.to_ascii_uppercase())
                .ok_or_else(|| format!("unknown timecycle '{id}'"))?;
            let global = &regions["GLOBAL"];
            let local = regions.get(&region.to_ascii_uppercase()).unwrap_or(global);
            for (key, samples) in &global.channels {
                let base = samples[before] + (samples[after] - samples[before]) * t;
                let local_value = local
                    .channels
                    .get(key)
                    .map(|s| s[before] + (s[after] - s[before]) * t)
                    .unwrap_or(base);
                *frame.channels.entry(key.clone()).or_default() +=
                    (base + (local_value - base) * region_blend) * weight / total;
            }
            // Newer regional datasets can define additional channels (e.g. seasonal FX).
            for (key, samples) in &local.channels {
                if global.channels.contains_key(key) {
                    continue;
                }
                *frame.channels.entry(key.clone()).or_default() +=
                    (samples[before] + (samples[after] - samples[before]) * t) * weight / total;
            }
        }
        Ok(frame)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn catalog() -> TimecycleCatalog {
        let v = serde_json::json!({"entries":[{"data":{"cycle":{"_attributes":{"name":"A"},"region":[{"_attributes":{"name":"GLOBAL"},"x":"0 10 20"},{"_attributes":{"name":"URBAN"},"x":"100 110 120"}]}}},{"data":{"cycle":{"_attributes":{"name":"B"},"region":[{"_attributes":{"name":"GLOBAL"},"x":"20 30 40"}]}}}]});
        TimecycleCatalog::parse(
            &v,
            vec![
                TimeSample {
                    name: "night".into(),
                    hour: 0.0,
                    duration: 4.0,
                },
                TimeSample {
                    name: "dawn".into(),
                    hour: 5.0,
                    duration: 0.0,
                },
                TimeSample {
                    name: "evening".into(),
                    hour: 22.0,
                    duration: 0.0,
                },
            ],
        )
        .unwrap()
    }
    #[test]
    fn hold_and_midnight() {
        let c = catalog();
        assert_eq!(c.sample_info(3.5), (0, 1, 0.0));
        assert!((c.sample_info(4.5).2 - 0.5).abs() < 1e-6);
        assert!((c.sample_info(23.0).2 - 0.5).abs() < 1e-6);
    }
    #[test]
    fn four_way_and_region() {
        let c = catalog();
        let weights = BTreeMap::from([("A".into(), 0.75), ("B".into(), 0.25)]);
        let f = c.evaluate(4.5, &weights, "URBAN", 0.5).unwrap();
        assert!((f.number("x", -1.0) - 47.5).abs() < 1e-6);
    }
    #[test]
    fn malformed_count_is_rejected() {
        let v = serde_json::json!({"entries":[]});
        assert!(TimecycleCatalog::parse(&v, vec![]).is_err());
    }
}
