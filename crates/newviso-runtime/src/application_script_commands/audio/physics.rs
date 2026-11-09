use super::*;

pub(super) fn parse_physics_audio_material(
    value: &str,
) -> Option<newviso_audio_client::ImpactMaterial> {
    use newviso_audio_client::ImpactMaterial;
    match value.trim().to_ascii_lowercase().as_str() {
        "generic" => Some(ImpactMaterial::Generic),
        "metal" => Some(ImpactMaterial::Metal),
        "glass" => Some(ImpactMaterial::Glass),
        "wood" => Some(ImpactMaterial::Wood),
        "stone" => Some(ImpactMaterial::Stone),
        "fabric" => Some(ImpactMaterial::Fabric),
        "rubber" => Some(ImpactMaterial::Rubber),
        _ => None,
    }
}

impl EngineApplication {
    /// Physical sound events are resolved after the Bullet step, regardless
    /// of whether the collision damaged anything. Only XVAG assets are emitted.
    pub(crate) fn route_physics_audio(
        &mut self,
        contacts: Vec<newviso_audio_client::PhysicsAudioContact>,
    ) {
        if contacts.is_empty() {
            return;
        }
        // Endorphin has first right of refusal for configured limb sounds.
        // It uses anatomical identifiers from the live ragdoll, never damage
        // or animation events. Claimed contacts cannot produce a generic copy.
        let (claimed, endorphin_emitted) = self.route_endorphin_audio(&contacts);
        if self.physics_audio_clips.is_empty() {
            return;
        }
        let audio = newviso_audio_client::AudioClient::new();
        let mut emitted = endorphin_emitted;
        let mut failed = 0usize;
        // Prioritize stronger impacts before consuming the voice/event budget.
        let mut contacts = contacts;
        contacts.sort_by(|a, b| {
            b.normal_impulse
                .total_cmp(&a.normal_impulse)
                .then_with(|| a.body_a.cmp(&b.body_a))
                .then_with(|| a.body_b.cmp(&b.body_b))
                .then_with(|| a.contact_group.cmp(&b.contact_group))
        });
        for contact in contacts.into_iter().take(4096) {
            if claimed.contains(&crate::application_endorphin::audio::contact_id(&contact)) {
                continue;
            }
            let Some(impact) = self.physics_audio_router.admit(contact) else {
                continue;
            };
            let Some(clip) = self.physics_audio_clips.get(&impact.material).or_else(|| {
                self.physics_audio_clips
                    .get(&newviso_audio_client::ImpactMaterial::Generic)
            }) else {
                continue;
            };
            if emitted >= 32 {
                break;
            }
            let mut request = newviso_audio_client::AudioPlayRequest::new(clip.clone());
            request.position = Some(impact.position);
            request.gain = (0.15 + impact.intensity * 1.25).clamp(0.0, 1.4);
            request.speed = (1.04 - impact.intensity * 0.08).clamp(0.9, 1.1);
            request.route = Some("sfx".to_owned());
            request.scope_id = Some(contact.body_a);
            match audio.play_xvag(&request) {
                Ok(ack) if ack.accepted => emitted += 1,
                Ok(_) | Err(_) => failed += 1,
            }
        }
        if failed > 0 {
            host::warn("newviso.audio.physics", format!(
                "{failed} XVAG contact playback request(s) failed; verify provider clip/spatial support and asset residency"
            ));
        }
    }
}

#[cfg(test)]
mod physics_material_tests {
    use super::*;
    #[test]
    fn acoustic_bindings_accept_known_categories_only() {
        assert_eq!(
            parse_physics_audio_material(" METAL "),
            Some(newviso_audio_client::ImpactMaterial::Metal)
        );
        assert!(parse_physics_audio_material("ice_as_metal").is_none());
    }
}
