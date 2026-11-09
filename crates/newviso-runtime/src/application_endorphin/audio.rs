//! Endorphin -> XVAG acoustic adapter. The Endorphin rig supplies limb identity;
//! Bullet supplies impulse/position; the audio provider owns decoding and mixing.
//! No audio callbacks, device access, or damage predicates live in this module.

use super::*;
use newviso_audio_client::{
    AudioClient, AudioPlayRequest, ImpactMaterial, PhysicsAudioContact, PhysicsAudioPolicy,
    PhysicsAudioRouter,
};

#[derive(Clone, Debug)]
struct LimbAudioSource<'a> {
    actor_entity: u64,
    part: std::borrow::Cow<'a, str>,
    region: std::borrow::Cow<'a, str>,
}

/// Retain borrowed model strings on the ordinary lower-case authored rig path.
fn lowercase_if_needed(value: &str) -> std::borrow::Cow<'_, str> {
    if value.bytes().all(|c| !c.is_ascii_uppercase()) {
        std::borrow::Cow::Borrowed(value)
    } else {
        std::borrow::Cow::Owned(value.to_ascii_lowercase())
    }
}

/// Authored per-region XVAG clips and independent contact-episode history.
#[derive(Debug)]
pub(crate) struct EndorphinAudioRuntime {
    // Region-first lookup avoids allocating a composite String key for each
    // candidate limb/material contact on every physics frame.
    clips: BTreeMap<String, BTreeMap<ImpactMaterial, Vec<String>>>,
    contacts: PhysicsAudioRouter,
}

impl Default for EndorphinAudioRuntime {
    fn default() -> Self {
        Self {
            clips: BTreeMap::new(),
            contacts: PhysicsAudioRouter::new(PhysicsAudioPolicy {
                minimum_impulse_ns: 3.0,
                minimum_normal_speed_mps: 0.30,
                ..PhysicsAudioPolicy::default()
            }),
        }
    }
}

impl EndorphinAudioRuntime {
    pub(crate) fn bind(
        &mut self,
        region: &str,
        material: ImpactMaterial,
        clip: &str,
    ) -> Result<(), String> {
        self.bind_variants(region, material, &[clip.to_owned()])
    }

    pub(crate) fn bind_variants(
        &mut self,
        region: &str,
        material: ImpactMaterial,
        clips: &[String],
    ) -> Result<(), String> {
        let region = validate_region(region)?;
        if clips.is_empty() || clips.len() > 16 {
            return Err("Endorphin XVAG bank must contain 1..=16 clips".to_owned());
        }
        let mut normalized = Vec::with_capacity(clips.len());
        for clip in clips {
            let clip = newviso_audio_client::normalize_xvag_uri(clip)?;
            if !normalized.contains(&clip) {
                normalized.push(clip);
            }
        }
        self.clips
            .entry(region)
            .or_default()
            .insert(material, normalized);
        self.contacts.clear();
        Ok(())
    }

    pub(crate) fn unbind(&mut self, region: &str, material: ImpactMaterial) -> Result<(), String> {
        let region = validate_region(region)?;
        if let std::collections::btree_map::Entry::Occupied(mut entry) = self.clips.entry(region) {
            entry.get_mut().remove(&material);
            if entry.get().is_empty() {
                entry.remove();
            }
        }
        self.contacts.clear();
        Ok(())
    }

    fn clip_for<'a>(
        &'a self,
        limb: &LimbAudioSource<'_>,
        material: ImpactMaterial,
    ) -> Option<&'a [String]> {
        // Three borrowed lookups instead of six allocated composite keys.
        for region in [limb.part.as_ref(), limb.region.as_ref(), "body"] {
            if let Some(bank) = self.clips.get(region) {
                if let Some(clips) = bank
                    .get(&material)
                    .or_else(|| bank.get(&ImpactMaterial::Generic))
                {
                    return Some(clips);
                }
            }
        }
        None
    }

    fn is_enabled(&self) -> bool {
        !self.clips.is_empty()
    }
}

fn validate_region(region: &str) -> Result<String, String> {
    let name = region.trim().to_ascii_lowercase();
    if name.is_empty()
        || name.len() > 64
        || !name
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_' || c == b'-')
    {
        return Err("Endorphin audio region must be a short ASCII identifier".to_owned());
    }
    Ok(name)
}

pub(crate) fn contact_id(contact: &PhysicsAudioContact) -> (u64, u64, u32) {
    (
        contact.body_a.min(contact.body_b),
        contact.body_a.max(contact.body_b),
        contact.contact_group,
    )
}

impl EngineApplication {
    /// Called once after the Bullet step, before the generic contact sounds.
    /// The return set suppresses generic sound duplicates for authored limb clips.
    pub(crate) fn route_endorphin_audio(
        &mut self,
        contacts: &[PhysicsAudioContact],
    ) -> (BTreeSet<(u64, u64, u32)>, usize) {
        if !self.endorphin_audio.is_enabled() || contacts.is_empty() {
            return (BTreeSet::new(), 0);
        }

        // Reconstructed only when there are authored Endorphin clips; no
        // dependency is added to the audio-thread or the physics backend.
        // Active ragdolls are typically a small subset of all pedestrians:
        // do not reserve 16 entries for every idle, non-articulated NPC.
        let mut limbs = std::collections::HashMap::<u64, LimbAudioSource<'_>>::new();
        for body in self.ped_physics.bodies.values() {
            let Some(rig) = body.endorphin.ragdoll.as_ref() else {
                continue;
            };
            for part in &rig.parts {
                let region = body
                    .endorphin
                    .model
                    .anatomy
                    .as_ref()
                    .and_then(|anatomy| anatomy.parts.get(&part.name))
                    .map(|anatomy| anatomy.region.as_str())
                    .unwrap_or("body");
                limbs.insert(
                    part.entity,
                    LimbAudioSource {
                        actor_entity: body.entity,
                        part: lowercase_if_needed(&part.name),
                        region: lowercase_if_needed(region),
                    },
                );
            }
        }
        if limbs.is_empty() {
            return (BTreeSet::new(), 0);
        }

        let listener = self.scene.focus_position();
        let mut candidates = Vec::new();
        let mut claimed = BTreeSet::new();
        for contact in contacts {
            let a = limbs.get(&contact.body_a);
            let b = limbs.get(&contact.body_b);
            // No synthetic body-on-body sounds from an articulated rig's own
            // joint stabilization; that contact is silent for generic audio too.
            if let (Some(a), Some(b)) = (a, b) {
                if a.actor_entity == b.actor_entity {
                    claimed.insert(contact_id(contact));
                    continue;
                }
            }
            let Some(limb) = a.or(b) else {
                continue;
            };
            let Some(clip) = self.endorphin_audio.clip_for(limb, contact.material) else {
                continue;
            };
            // An explicit Endorphin binding owns this contact even if it is
            // too quiet to produce a voice (no doubled generic collision).
            claimed.insert(contact_id(contact));
            let distance2 = contact
                .position
                .iter()
                .zip(listener)
                .map(|(p, l)| (p - l) * (p - l))
                .sum::<f32>();
            if !distance2.is_finite() || distance2 > 90.0 * 90.0 {
                continue;
            }
            let importance =
                contact.normal_impulse.max(0.0).ln_1p() / (1.0 + distance2.sqrt() * 0.08);
            let seed = contact.body_a.rotate_left(9)
                ^ contact.body_b.rotate_left(27)
                ^ limb.actor_entity
                ^ contact.contact_group as u64
                ^ contact.seconds.to_bits();
            let variant = (seed as usize) % clip.len();
            candidates.push((
                importance,
                limb.actor_entity,
                clip[variant].clone(),
                *contact,
            ));
        }
        // Strong, nearby impacts take precedence over remote, insignificant ones.
        candidates.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        let mut per_actor = BTreeMap::<u64, usize>::new();
        let mut accepted = 0usize;
        let mut rejected = 0usize;
        let audio = AudioClient::new();
        for (_, actor, clip, contact) in candidates {
            if accepted >= 24 {
                break;
            }
            if per_actor.get(&actor).copied().unwrap_or(0) >= 2 {
                continue;
            }
            let Some(impact) = self.endorphin_audio.contacts.admit(contact) else {
                continue;
            };
            let mut request = AudioPlayRequest::new(clip);
            request.position = Some(impact.position);
            request.route = Some("sfx".to_owned());
            request.scope_id = Some(actor);
            request.gain = (0.12 + impact.intensity * 1.1).clamp(0.0, 1.25);
            request.speed = (0.96 + (contact.contact_group % 9) as f32 * 0.013).clamp(0.90, 1.08);
            request.seed = Some(
                actor.rotate_left(17)
                    ^ contact.body_a.rotate_left(9)
                    ^ contact.body_b
                    ^ contact.contact_group as u64
                    ^ contact.seconds.to_bits(),
            );
            match audio.play_xvag(&request) {
                Ok(ack) if ack.accepted => {
                    accepted += 1;
                    *per_actor.entry(actor).or_default() += 1;
                }
                Ok(_) | Err(_) => rejected += 1,
            }
        }
        if rejected > 0 {
            host::warn(
                "newviso.audio.endorphin",
                format!("{rejected} Endorphin XVAG requests rejected; check asset/provider availability"),
            );
        }
        (claimed, accepted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limb(part: &str, region: &str) -> LimbAudioSource<'static> {
        LimbAudioSource {
            actor_entity: 88,
            part: std::borrow::Cow::Owned(part.into()),
            region: std::borrow::Cow::Owned(region.into()),
        }
    }

    #[test]
    fn explicit_limb_clip_precedes_region_and_generic_fallback() {
        let mut audio = EndorphinAudioRuntime::default();
        audio
            .bind("body", ImpactMaterial::Generic, "audio/body.xvag")
            .unwrap();
        audio
            .bind("arm", ImpactMaterial::Generic, "audio/arm.xvag")
            .unwrap();
        audio
            .bind("left_hand", ImpactMaterial::Metal, "audio/hand_metal.xvag")
            .unwrap();
        assert_eq!(
            audio
                .clip_for(&limb("left_hand", "arm"), ImpactMaterial::Metal)
                .unwrap(),
            ["audio/hand_metal.xvag"]
        );
        assert_eq!(
            audio
                .clip_for(&limb("right_hand", "arm"), ImpactMaterial::Stone)
                .unwrap(),
            ["audio/arm.xvag"]
        );
        assert_eq!(
            audio
                .clip_for(&limb("left_foot", "leg"), ImpactMaterial::Wood)
                .unwrap(),
            ["audio/body.xvag"]
        );
        audio.unbind("arm", ImpactMaterial::Generic).unwrap();
        assert_eq!(
            audio
                .clip_for(&limb("right_hand", "arm"), ImpactMaterial::Stone)
                .unwrap(),
            ["audio/body.xvag"]
        );
    }

    #[test]
    fn standard_rig_identifiers_borrow_without_allocating() {
        assert!(matches!(
            lowercase_if_needed("left_hand"),
            std::borrow::Cow::Borrowed(_)
        ));
        assert_eq!(lowercase_if_needed("LEFT_HAND"), "left_hand");
        let mut audio = EndorphinAudioRuntime::default();
        audio
            .bind("body", ImpactMaterial::Generic, "audio/body.xvag")
            .unwrap();
        audio
            .bind("arm", ImpactMaterial::Metal, "audio/arm_metal.xvag")
            .unwrap();
        let source = limb("left_hand", "arm");
        assert_eq!(
            audio.clip_for(&source, ImpactMaterial::Metal).unwrap(),
            ["audio/arm_metal.xvag"]
        );
        audio.unbind("arm", ImpactMaterial::Metal).unwrap();
        assert_eq!(
            audio.clip_for(&source, ImpactMaterial::Metal).unwrap(),
            ["audio/body.xvag"]
        );
        audio.unbind("body", ImpactMaterial::Generic).unwrap();
        assert!(!audio.is_enabled());
    }

    #[test]
    fn bindings_require_native_xvag_and_safe_regions() {
        let mut audio = EndorphinAudioRuntime::default();
        assert!(audio
            .bind("arm", ImpactMaterial::Generic, "audio/old.yscd")
            .is_err());
        assert!(audio
            .bind("../arm", ImpactMaterial::Generic, "audio/arm.xvag")
            .is_err());
        assert!(audio
            .bind("   ", ImpactMaterial::Generic, "audio/arm.xvag")
            .is_err());
    }

    #[test]
    fn authored_sound_banks_are_bounded_deduplicated_and_validated() {
        let mut audio = EndorphinAudioRuntime::default();
        audio
            .bind_variants(
                "leg",
                ImpactMaterial::Stone,
                &[
                    "audio/fall1.xvag".into(),
                    "audio/fall1.xvag".into(),
                    "audio/fall2.xvag".into(),
                ],
            )
            .unwrap();
        let bank = audio
            .clip_for(&limb("left_foot", "leg"), ImpactMaterial::Stone)
            .unwrap()
            .to_vec();
        assert_eq!(bank, ["audio/fall1.xvag", "audio/fall2.xvag"]);
        assert!(audio
            .bind_variants("leg", ImpactMaterial::Generic, &[])
            .is_err());
        assert!(audio
            .bind_variants(
                "leg",
                ImpactMaterial::Generic,
                &vec!["audio/a.xvag".into(); 17]
            )
            .is_err());
        assert_eq!(
            audio
                .clip_for(&limb("left_foot", "leg"), ImpactMaterial::Stone)
                .unwrap(),
            bank
        );
    }

    #[test]
    fn contact_ids_are_canonical_for_any_body_order() {
        let contact = PhysicsAudioContact {
            body_a: 42,
            body_b: 7,
            contact_group: 3,
            position: [0.0; 3],
            normal_impulse: 90.0,
            effective_mass_kg: 2.0,
            normal_speed_mps: Some(3.0),
            material: ImpactMaterial::Generic,
            seconds: 1.0,
        };
        let mut reversed = contact;
        std::mem::swap(&mut reversed.body_a, &mut reversed.body_b);
        assert_eq!(contact_id(&contact), contact_id(&reversed));
    }

    #[test]
    fn quiet_solver_contacts_are_discarded_but_limb_impacts_sound() {
        let mut audio = EndorphinAudioRuntime::default();
        let mut contact = PhysicsAudioContact {
            body_a: 42,
            body_b: 7,
            contact_group: 3,
            position: [0.0; 3],
            normal_impulse: 5.0,
            effective_mass_kg: 1.5,
            normal_speed_mps: Some(0.02),
            material: ImpactMaterial::Generic,
            seconds: 1.0,
        };
        assert!(audio.contacts.admit(contact).is_none());
        contact.seconds += 0.2;
        contact.normal_speed_mps = Some(3.0);
        assert!(audio.contacts.admit(contact).is_some());
    }
}
