use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Authored marker weights and admitted samples, independent of body mass.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryMatchProfile {
    pub joints: Vec<RecoveryMatchJoint>,
    /// Keys are the installed directional get-up clip references.
    pub clips: BTreeMap<String, RecoveryMatchClip>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryMatchJoint {
    pub joint: String,
    pub weight: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryMatchClip {
    pub sample_times_seconds: Vec<f32>,
    /// Multiplier on the pose error. A smaller value favors this item.
    #[serde(default = "unit_weight")]
    pub score_multiplier: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blend_seconds: Option<f32>,
}

fn unit_weight() -> f32 {
    1.0
}

impl RecoveryMatchProfile {
    pub fn validate(&self) -> Result<(), String> {
        let mut names = BTreeSet::new();
        if self.joints.len() > 512
            || self.joints.iter().filter(|m| m.weight > 0.0).count() < 3
            || self.joints.iter().any(|m| {
                m.joint.trim().is_empty()
                    || !m.weight.is_finite()
                    || m.weight < 0.0
                    || !names.insert(m.joint.to_ascii_lowercase())
            })
            || self.clips.is_empty()
        {
            return Err("invalid recovery match joint weights or empty clip filter".into());
        }
        for (clip_ref, item) in &self.clips {
            let times = &item.sample_times_seconds;
            if !clip_ref
                .split_once('@')
                .is_some_and(|(file, clip)| !file.trim().is_empty() && !clip.trim().is_empty())
                || times.is_empty()
                || times.len() > 2048
                || times.iter().any(|t| !t.is_finite() || *t < 0.0)
                || times.windows(2).any(|w| w[0] >= w[1])
                || !item.score_multiplier.is_finite()
                || item.score_multiplier <= 0.0
                || item
                    .blend_seconds
                    .is_some_and(|t| !t.is_finite() || t <= 0.0)
            {
                return Err(format!("invalid recovery match item {clip_ref}"));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile() -> RecoveryMatchProfile {
        serde_json::from_str(
            r#"{
            "joints":[{"joint":"pelvis","weight":3},{"joint":"arm","weight":1},
                      {"joint":"foot","weight":2},{"joint":"unused","weight":0}],
            "clips":{"getup.ycd@back":{"sample_times_seconds":[0.1,0.35]}}
        }"#,
        )
        .unwrap()
    }

    #[test]
    fn profile_accepts_zero_weights_and_defaults_to_neutral_clip_bias() {
        let profile = profile();
        assert!(profile.validate().is_ok());
        assert_eq!(profile.clips["getup.ycd@back"].score_multiplier, 1.0);
    }

    #[test]
    fn profile_rejects_duplicate_joints_and_bad_sample_or_blend_data() {
        let mut invalid = profile();
        invalid.joints[3].joint = "ARM".into();
        assert!(invalid.validate().is_err());
        let mut invalid = profile();
        let item = invalid.clips.values_mut().next().unwrap();
        item.sample_times_seconds = vec![0.2, 0.2];
        assert!(invalid.validate().is_err());
        let mut invalid = profile();
        let item = invalid.clips.values_mut().next().unwrap();
        item.blend_seconds = Some(f32::NAN);
        assert!(invalid.validate().is_err());
    }
}
