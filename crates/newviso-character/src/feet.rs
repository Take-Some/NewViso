//! Provider-independent anatomy contract for terrain-aware supporting limbs.
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
pub const MAX_SUPPORT_LIMBS: usize = 32;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FootLeg {
    pub hip: String,
    pub knee: String,
    pub foot: String,
    pub toe: String,
    pub sole_height: f32,
    #[serde(default)]
    pub sole_half_width: Option<f32>,
    #[serde(default)]
    pub heel_length: Option<f32>,
}
pub fn validate_foot_profile(legs: &[FootLeg]) -> Result<(), String> {
    if legs.is_empty() || legs.len() > MAX_SUPPORT_LIMBS {
        return Err("foot profile needs 1..=32 supporting limb chains".into());
    }
    let mut driven = BTreeSet::new();
    for leg in legs {
        for name in [&leg.hip, &leg.knee, &leg.foot, &leg.toe] {
            if name.trim().is_empty() || name.len() > 128 {
                return Err("invalid supporting limb joint name".into());
            }
        }
        for name in [&leg.hip, &leg.knee, &leg.foot] {
            if !driven.insert(name.to_ascii_lowercase()) {
                return Err("supporting limb chains may not drive the same joint twice".into());
            }
        }
        if !leg.sole_height.is_finite()
            || leg.sole_height <= 0.0
            || [leg.sole_half_width, leg.heel_length]
                .into_iter()
                .flatten()
                .any(|v| !v.is_finite() || v <= 0.0)
        {
            return Err("sole dimensions must be finite and positive".into());
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn leg(i: usize) -> FootLeg {
        FootLeg {
            hip: format!("support{i}.upper"),
            knee: format!("support{i}.lower"),
            foot: format!("support{i}.end"),
            toe: format!("support{i}.front"),
            sole_height: 0.05,
            sole_half_width: Some(0.04),
            heel_length: Some(0.03),
        }
    }
    #[test]
    fn arbitrary_names_and_limb_counts_are_anatomy_data() {
        for n in [1, 2, 4, 6, 32] {
            assert!(validate_foot_profile(&(0..n).map(leg).collect::<Vec<_>>()).is_ok());
        }
        assert!(validate_foot_profile(&[]).is_err());
        assert!(validate_foot_profile(&(0..33).map(leg).collect::<Vec<_>>()).is_err());
    }
    #[test]
    fn overlapping_drivers_and_nonphysical_dimensions_are_rejected() {
        let mut a = vec![leg(0), leg(1)];
        a[1].knee = a[0].foot.to_uppercase();
        assert!(validate_foot_profile(&a).is_err());
        let mut a = leg(0);
        a.sole_half_width = Some(f32::NAN);
        assert!(validate_foot_profile(&[a]).is_err());
    }
}
