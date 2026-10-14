use serde::Deserialize;
use std::collections::BTreeMap;

/// Independent cloud curves use fourteen hourly keys plus the 24:00 seam.
#[derive(Clone, Debug, Deserialize)]
pub struct CloudKeyframeCatalog {
    pub presets: BTreeMap<String, BTreeMap<String, Vec<[f32; 5]>>>,
    #[serde(default)]
    pub cloud_hat_count: usize,
    #[serde(default)]
    pub cloud_lists: BTreeMap<String, super::CloudHatList>,
}
impl CloudKeyframeCatalog {
    pub fn validate(&self) -> Result<(), String> {
        if self.presets.is_empty() {
            return Err("empty cloud keyframe catalog".into());
        }
        if self.cloud_hat_count > 1024 {
            return Err("too many cloud hats".into());
        }
        for (name, list) in &self.cloud_lists {
            if !self.presets.contains_key(name) {
                return Err("CloudList has no keyframe preset".into());
            }
            list.validate(self.cloud_hat_count)?;
        }
        for channels in self.presets.values() {
            if channels.is_empty() {
                return Err("empty cloud preset".into());
            }
            for keys in channels.values() {
                if keys.len() < 2
                    || keys.first().unwrap()[0] != 0.0
                    || keys.last().unwrap()[0] != 24.0
                    || keys.iter().flatten().any(|v| !v.is_finite())
                    || keys.windows(2).any(|w| w[0][0] >= w[1][0])
                {
                    return Err("invalid cloud curve or missing midnight seam".into());
                }
            }
        }
        Ok(())
    }
    pub fn evaluate(
        &self,
        hours: f64,
        weights: &BTreeMap<String, f32>,
    ) -> Result<BTreeMap<String, [f32; 4]>, String> {
        if !hours.is_finite() || weights.values().any(|w| !w.is_finite() || *w < 0.0) {
            return Err("invalid cloud evaluation".into());
        }
        let total: f32 = weights.values().sum();
        if total <= 0.0 || !total.is_finite() {
            return Err("empty cloud weights".into());
        }
        let hour = hours.rem_euclid(24.0) as f32;
        let mut out = BTreeMap::<String, [f32; 4]>::new();
        for (id, weight) in weights {
            if *weight <= 0.0 {
                continue;
            }
            let channels = self
                .presets
                .get(&id.to_ascii_uppercase())
                .ok_or_else(|| format!("unknown cloud preset '{id}'"))?;
            for (name, keys) in channels {
                let before = keys
                    .iter()
                    .rposition(|k| k[0] <= hour)
                    .unwrap_or(0)
                    .min(keys.len() - 2);
                let a = keys[before];
                let b = keys[before + 1];
                let t = ((hour - a[0]) / (b[0] - a[0])).clamp(0.0, 1.0);
                let result = out.entry(name.clone()).or_default();
                for i in 0..4 {
                    result[i] += (a[i + 1] + (b[i + 1] - a[i + 1]) * t) * weight / total;
                }
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cloud_curves_interpolate_at_four_and_wrap_at_midnight() {
        let c = CloudKeyframeCatalog {
            cloud_hat_count: 0,
            cloud_lists: BTreeMap::new(),
            presets: BTreeMap::from([(
                "LIGHT".into(),
                BTreeMap::from([(
                    "color".into(),
                    vec![
                        [0.0, 0.0, 0.0, 0.0, 1.0],
                        [4.0, 1.0, 1.0, 1.0, 1.0],
                        [24.0, 0.0, 0.0, 0.0, 1.0],
                    ],
                )]),
            )]),
        };
        c.validate().unwrap();
        let w = BTreeMap::from([("LIGHT".into(), 1.0)]);
        assert_eq!(c.evaluate(2.0, &w).unwrap()["color"], [0.5, 0.5, 0.5, 1.0]);
        assert_eq!(c.evaluate(24.0, &w).unwrap()["color"], [0.0, 0.0, 0.0, 1.0]);
        assert!((c.evaluate(23.0, &w).unwrap()["color"][0] - 0.05).abs() < 1e-6);
    }
}
