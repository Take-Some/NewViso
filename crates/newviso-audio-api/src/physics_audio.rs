//! Engine-owned contact-to-audio admission. This module never touches the
//! audio device or loads clips; it converts physics observations into bounded,
//! repeatable sound-event decisions for the native audio provider.
//!
//! The caller owns the final YSCD cue and routes it through engine.audio.
//! Damage thresholds must not be reused for audio: non-damaging impacts sound.

use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ImpactMaterial {
    Generic,
    Metal,
    Glass,
    Wood,
    Stone,
    Fabric,
    Rubber,
}

#[derive(Clone, Copy, Debug)]
pub struct PhysicsAudioContact {
    pub body_a: u64,
    pub body_b: u64,
    /// Stable sub-contact identity (e.g. a vehicle fragment index).
    pub contact_group: u32,
    pub position: [f32; 3],
    /// Normal impulse in N*s, not damage points or velocity.
    pub normal_impulse: f32,
    /// Effective mass of the contact pair, in kg.
    pub effective_mass_kg: f32,
    /// Optional magnitude of relative velocity along the contact normal, m/s.
    pub normal_speed_mps: Option<f32>,
    pub material: ImpactMaterial,
    pub seconds: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AdmittedImpact {
    pub position: [f32; 3],
    /// Normalized perceptual amplitude proxy in [0, 1].
    pub intensity: f32,
    pub material: ImpactMaterial,
    pub impulse_ns: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct PhysicsAudioPolicy {
    /// Reject minor numerical solver impulses.
    pub minimum_impulse_ns: f32,
    /// Known sub-threshold approach speeds are resting/support contacts,
    /// even if Bullet reports a large persistent support impulse.
    pub minimum_normal_speed_mps: f32,
    /// A contact silent for this long begins a fresh episode.
    pub episode_gap_seconds: f64,
    /// Do not retrigger while peak impulse stays within this fractional rise.
    pub retrigger_peak_fraction: f32,
    pub retrigger_cooldown_seconds: f64,
}

impl Default for PhysicsAudioPolicy {
    fn default() -> Self {
        Self {
            minimum_impulse_ns: 15.0,
            minimum_normal_speed_mps: 0.25,
            episode_gap_seconds: 0.18,
            retrigger_peak_fraction: 0.30,
            retrigger_cooldown_seconds: 0.07,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ContactKey(u64, u64, u32);

#[derive(Clone, Copy, Debug)]
struct ContactEpisode {
    last_seen_seconds: f64,
    peak_impulse_ns: f32,
    last_emitted_seconds: f64,
}

/// A simulation-thread event router. It is not used from the audio callback.
/// The caller owns one instance per scene or independently simulated entity.
#[derive(Clone, Debug, Default)]
pub struct PhysicsAudioRouter {
    policy: PhysicsAudioPolicy,
    episodes: BTreeMap<ContactKey, ContactEpisode>,
}

impl PhysicsAudioRouter {
    const MAX_EPISODES: usize = 1024;

    pub fn new(policy: PhysicsAudioPolicy) -> Self {
        Self {
            policy,
            episodes: BTreeMap::new(),
        }
    }

    pub fn clear(&mut self) {
        self.episodes.clear();
    }

    pub fn tracked_contacts(&self) -> usize {
        self.episodes.len()
    }

    /// One admission per contact episode, except a sufficiently stronger
    /// secondary impact after the retrigger cooldown.
    ///
    /// This does not enqueue PCM or play audio. The event may be dropped later
    /// by the provider's voice-budget and occlusion arbitration.
    pub fn admit(&mut self, input: PhysicsAudioContact) -> Option<AdmittedImpact> {
        if !input.seconds.is_finite()
            || !input.normal_impulse.is_finite()
            || !input.effective_mass_kg.is_finite()
            || input.position.iter().any(|c| !c.is_finite())
            || input.normal_speed_mps.is_some_and(|v| !v.is_finite())
            || input.normal_impulse <= self.policy.minimum_impulse_ns
            || input
                .normal_speed_mps
                .is_some_and(|v| v.abs() < self.policy.minimum_normal_speed_mps)
            || input.effective_mass_kg <= 0.0
        {
            return None;
        }

        let impulse = input.normal_impulse.max(0.0);
        let (a, b) = if input.body_a <= input.body_b {
            (input.body_a, input.body_b)
        } else {
            (input.body_b, input.body_a)
        };
        let key = ContactKey(a, b, input.contact_group);
        let policy = self.policy;

        // The cap prevents unbounded growth when the world generates new
        // object ids. Pruning is done on the simulation thread, not the RT
        // callback, and only when approaching capacity.
        if self.episodes.len() >= Self::MAX_EPISODES && !self.episodes.contains_key(&key) {
            self.episodes.retain(|_, episode| {
                input.seconds >= episode.last_seen_seconds
                    && input.seconds - episode.last_seen_seconds <= policy.episode_gap_seconds
            });
            if self.episodes.len() >= Self::MAX_EPISODES {
                if let Some(oldest) = self
                    .episodes
                    .iter()
                    .min_by(|a, b| {
                        a.1.last_seen_seconds
                            .total_cmp(&b.1.last_seen_seconds)
                            .then_with(|| a.0.cmp(b.0))
                    })
                    .map(|(key, _)| *key)
                {
                    self.episodes.remove(&oldest);
                }
            }
        }

        let mut emit = true;
        match self.episodes.get_mut(&key) {
            Some(episode)
                if input.seconds >= episode.last_seen_seconds
                    && input.seconds - episode.last_seen_seconds <= policy.episode_gap_seconds =>
            {
                let rising =
                    impulse > episode.peak_impulse_ns * (1.0 + policy.retrigger_peak_fraction);
                let cooled = input.seconds - episode.last_emitted_seconds
                    >= policy.retrigger_cooldown_seconds;
                emit = rising && cooled;
                episode.last_seen_seconds = input.seconds;
                episode.peak_impulse_ns = episode.peak_impulse_ns.max(impulse);
                if emit {
                    episode.last_emitted_seconds = input.seconds;
                }
            }
            _ => {
                self.episodes.insert(
                    key,
                    ContactEpisode {
                        last_seen_seconds: input.seconds,
                        peak_impulse_ns: impulse,
                        last_emitted_seconds: input.seconds,
                    },
                );
            }
        }
        if !emit {
            return None;
        }

        // Impulse per mass is a conservative proxy when the solver cannot
        // supply relative normal speed. Explicit speed adds perceptual energy.
        let speed_proxy = impulse / input.effective_mass_kg.max(1.0);
        let normal_speed = input
            .normal_speed_mps
            .map(|v| v.abs())
            .unwrap_or(speed_proxy);
        let intensity = ((speed_proxy * 0.24 + normal_speed * 0.12) / 2.5)
            .sqrt()
            .clamp(0.0, 1.0);
        Some(AdmittedImpact {
            position: input.position,
            intensity,
            material: input.material,
            impulse_ns: impulse,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contact(seconds: f64, impulse: f32) -> PhysicsAudioContact {
        PhysicsAudioContact {
            body_a: 9,
            body_b: 14,
            contact_group: 2,
            position: [1.0, 2.0, 3.0],
            normal_impulse: impulse,
            effective_mass_kg: 100.0,
            normal_speed_mps: None,
            material: ImpactMaterial::Metal,
            seconds,
        }
    }

    #[test]
    fn resting_support_does_not_sound_like_a_collision() {
        let mut router = PhysicsAudioRouter::default();
        let mut support = contact(1.0, 300.0);
        support.normal_speed_mps = Some(0.04);
        assert!(router.admit(support).is_none());
        support.seconds += 0.1;
        support.normal_speed_mps = Some(2.0);
        assert!(router.admit(support).is_some());
    }

    #[test]
    fn solver_contacts_do_not_emit_a_sound_every_tick() {
        let mut router = PhysicsAudioRouter::default();
        assert!(router.admit(contact(1.0, 200.0)).is_some());
        for i in 1..15 {
            assert!(router
                .admit(contact(1.0 + i as f64 / 60.0, 200.0))
                .is_none());
        }
    }

    #[test]
    fn genuinely_harder_hit_and_new_contact_retrigger() {
        let mut router = PhysicsAudioRouter::default();
        assert!(router.admit(contact(1.0, 100.0)).is_some());
        assert!(router.admit(contact(1.04, 160.0)).is_none()); // cooldown
        assert!(router.admit(contact(1.09, 220.0)).is_some());
        assert!(router.admit(contact(1.4, 220.0)).is_some()); // separation
    }

    #[test]
    fn unordered_pair_identity_and_contact_group_are_respected() {
        let mut router = PhysicsAudioRouter::default();
        assert!(router.admit(contact(1.0, 200.0)).is_some());
        let mut swapped = contact(1.01, 200.0);
        std::mem::swap(&mut swapped.body_a, &mut swapped.body_b);
        assert!(router.admit(swapped).is_none());
        swapped.contact_group += 1;
        assert!(router.admit(swapped).is_some());
    }

    #[test]
    fn reject_nan_and_solver_noise() {
        let mut router = PhysicsAudioRouter::default();
        assert!(router.admit(contact(0.0, f32::NAN)).is_none());
        assert!(router.admit(contact(0.0, 1.0)).is_none());
        let mut invalid = contact(0.0, 200.0);
        invalid.position[0] = f32::INFINITY;
        assert!(router.admit(invalid).is_none());
        assert_eq!(router.tracked_contacts(), 0);
    }

    #[test]
    fn larger_impulses_mean_louder_impacts() {
        let mut small = PhysicsAudioRouter::default();
        let mut big = PhysicsAudioRouter::default();
        let a = small.admit(contact(1.0, 30.0)).unwrap();
        let b = big.admit(contact(1.0, 300.0)).unwrap();
        assert!(b.intensity > a.intensity);
        assert!(b.intensity <= 1.0);
    }

    #[test]
    fn capacity_is_bounded_for_unique_contacts() {
        let mut router = PhysicsAudioRouter::default();
        for id in 0..1100 {
            let mut c = contact(1.0, 200.0);
            c.body_b = id + 100;
            assert!(router.admit(c).is_some());
        }
        assert!(router.tracked_contacts() <= PhysicsAudioRouter::MAX_EPISODES);
    }

    #[test]
    fn clear_removes_contact_history() {
        let mut router = PhysicsAudioRouter::default();
        assert!(router.admit(contact(1.0, 200.0)).is_some());
        router.clear();
        assert!(router.admit(contact(1.01, 200.0)).is_some());
    }
}
