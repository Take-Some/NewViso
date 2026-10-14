use serde::{Deserialize, Serialize};

/// Original CloudList bit N addresses cloud fragment N. The last bit is NoCloud.
#[derive(Clone, Debug, Deserialize)]
pub struct CloudHatList {
    pub bit_count: usize,
    pub bit_words: Vec<u32>,
    pub probability: Vec<u32>,
}
impl CloudHatList {
    pub fn validate(&self, cloud_count: usize) -> Result<(), String> {
        if self.bit_count != cloud_count + 1
            || self.bit_words.len() < self.bit_count.div_ceil(32)
            || self.probability.len() != self.bit_count
        {
            return Err("invalid CloudList mask or probability count".into());
        }
        Ok(())
    }
    fn active(&self, index: usize) -> bool {
        self.bit_words[index / 32] & (1 << (index % 32)) != 0
    }
    pub fn select(&self, roll: u64, cloud_count: usize) -> usize {
        let total: u64 = (0..self.bit_count)
            .filter(|&i| self.active(i))
            .map(|i| u64::from(self.probability[i]))
            .sum();
        if total == 0 {
            return cloud_count;
        }
        let selector = roll % total;
        let mut cumulative = 0;
        for i in 0..self.bit_count {
            if self.active(i) {
                cumulative += u64::from(self.probability[i]);
                if selector < cumulative {
                    return i;
                }
            }
        }
        cloud_count
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CloudHatSelection {
    pub index: Option<usize>,
    pub key: Option<(String, usize)>,
    pub random_state: u64,
}
impl Default for CloudHatSelection {
    fn default() -> Self {
        Self::new(1)
    }
}
impl CloudHatSelection {
    pub fn new(seed: u64) -> Self {
        Self {
            index: None,
            key: None,
            random_state: seed.max(1),
        }
    }
    pub fn validate(&self, cloud_count: usize) -> Result<(), String> {
        if self.random_state == 0
            || self.index.is_some_and(|i| i > cloud_count)
            || self.index.is_some() != self.key.is_some()
        {
            return Err("invalid saved CloudHat selection".into());
        }
        Ok(())
    }
    pub fn update(
        &mut self,
        catalog: &super::CloudKeyframeCatalog,
        preset: &str,
        cycle_index: usize,
        weather: &str,
    ) -> Result<bool, String> {
        self.validate(catalog.cloud_hat_count)?;
        let preset = preset.to_ascii_uppercase();
        let key = (
            format!("{preset}:{}", weather.to_ascii_uppercase()),
            cycle_index,
        );
        if self.key.as_ref() == Some(&key) {
            return Ok(false);
        }
        let mut random = self.random_state;
        random ^= random << 13;
        random ^= random >> 7;
        random ^= random << 17;
        self.random_state = random;
        self.index = Some(
            catalog
                .cloud_lists
                .get(&preset)
                .map_or(catalog.cloud_hat_count, |list| {
                    list.select(random, catalog.cloud_hat_count)
                }),
        );
        self.key = Some(key);
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cloud_list_obeys_bits_weights_and_no_cloud_sentinel() {
        let list = CloudHatList {
            bit_count: 4,
            bit_words: vec![0b1101],
            probability: vec![2, 100, 0, 3],
        };
        list.validate(3).unwrap();
        assert_eq!(
            (0..5).map(|r| list.select(r, 3)).collect::<Vec<_>>(),
            vec![0, 0, 3, 3, 3]
        );
        let empty = CloudHatList {
            bit_count: 4,
            bit_words: vec![0],
            probability: vec![1; 4],
        };
        assert_eq!(empty.select(99, 3), 3);
    }
    #[test]
    fn selected_cloud_is_stable_across_frames_and_checkpoint_restore() {
        use std::collections::BTreeMap;
        let catalog = super::super::CloudKeyframeCatalog {
            presets: BTreeMap::new(),
            cloud_hat_count: 3,
            cloud_lists: BTreeMap::from([(
                "LIGHT".into(),
                CloudHatList {
                    bit_count: 4,
                    bit_words: vec![0b1111],
                    probability: vec![1; 4],
                },
            )]),
        };
        let mut state = CloudHatSelection::new(12345);
        assert!(state.update(&catalog, "LIGHT", 0, "CLEAR").unwrap());
        let before = state.random_state;
        assert!(!state.update(&catalog, "LIGHT", 0, "CLEAR").unwrap());
        assert_eq!(state.random_state, before);
        let mut restored: CloudHatSelection =
            serde_json::from_value(serde_json::to_value(&state).unwrap()).unwrap();
        assert!(!restored.update(&catalog, "LIGHT", 0, "CLEAR").unwrap());
        assert!(state.update(&catalog, "LIGHT", 1, "CLEAR").unwrap());
        assert!(restored.update(&catalog, "LIGHT", 1, "CLEAR").unwrap());
        assert_eq!(restored.index, state.index);
        assert_eq!(restored.random_state, state.random_state);
    }
}
