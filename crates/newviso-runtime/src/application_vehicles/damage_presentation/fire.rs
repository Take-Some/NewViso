use super::*;

impl EngineApplication {
    pub(crate) fn advance_vehicle_part_fires(&mut self, entity: u64) {
        let elapsed = self.elapsed_seconds;
        let Some(binding) = self.vehicle_presentations.get_mut(&entity) else {
            return;
        };
        let dt = (elapsed - binding.last_damage_update_seconds).clamp(0.0, 0.1) as f32;
        binding.last_damage_update_seconds = elapsed;
        if dt <= 0.0 {
            return;
        }

        for part in &mut binding.parts {
            if part.fire_intensity <= 0.0 || part.detached_entity.is_some() || !part.visible {
                continue;
            }
            let intensity = part.fire_intensity.clamp(0.0, 1.0);
            part.damage = (part.damage + dt * (0.018 + intensity * 0.055)).clamp(0.0, 1.0);
            part.fire_remaining_seconds = (part.fire_remaining_seconds - dt).max(0.0);
            if part.fire_remaining_seconds <= 0.0 {
                part.fire_intensity = (part.fire_intensity - dt * 0.28).max(0.0);
            }
            part.presentation_override = true;
        }

        // Fire spreads only through fragment hierarchy adjacency. This keeps a
        // burning door local instead of turning the whole vehicle into one health blob.
        let sources = binding
            .parts
            .iter()
            .filter(|part| {
                part.fire_intensity >= 0.68
                    && part.damage >= 0.32
                    && part.visible
                    && part.detached_entity.is_none()
            })
            .map(|part| (part.index, part.parent_part_index, part.fire_intensity))
            .collect::<Vec<_>>();
        let mut ignite = Vec::<(u32, f32)>::new();
        for (source_index, source_parent, intensity) in sources {
            for target in &binding.parts {
                if target.fire_intensity > 0.25
                    || !target.visible
                    || target.detached_entity.is_some()
                    || matches!(
                        target.role,
                        ModelFragmentPartRole::Glass | ModelFragmentPartRole::Light
                    )
                {
                    continue;
                }
                let adjacent = target.parent_part_index == Some(source_index)
                    || source_parent == Some(target.index);
                if adjacent {
                    ignite.push((target.index, intensity * 0.52));
                }
            }
        }
        for (index, intensity) in ignite {
            if let Some(part) = binding.parts.iter_mut().find(|part| part.index == index) {
                part.fire_intensity = part.fire_intensity.max(intensity.clamp(0.0, 0.72));
                part.fire_remaining_seconds = part.fire_remaining_seconds.max(4.5);
                part.presentation_override = true;
            }
        }
    }

    pub(crate) fn apply_vehicle_fire_event_presentation(
        &mut self,
        entity: u64,
        kind: VehicleEventKind,
    ) {
        let Some(binding) = self.vehicle_presentations.get_mut(&entity) else {
            return;
        };
        match kind {
            VehicleEventKind::EngineFireStarted => {
                let preferred = binding
                    .parts
                    .iter()
                    .position(|part| {
                        part.visible
                            && part.detached_entity.is_none()
                            && (part.name_lower.contains("engine")
                                || part.name_lower.contains("overheat"))
                    })
                    .or_else(|| {
                        binding.parts.iter().position(|part| {
                            part.role == ModelFragmentPartRole::Bonnet
                                && part.visible
                                && part.detached_entity.is_none()
                        })
                    })
                    .or_else(|| {
                        binding
                            .parts
                            .iter()
                            .enumerate()
                            .filter(|(_, part)| {
                                part.visible
                                    && part.detached_entity.is_none()
                                    && matches!(
                                        part.role,
                                        ModelFragmentPartRole::Body
                                            | ModelFragmentPartRole::BodyPanel
                                            | ModelFragmentPartRole::Extra
                                    )
                            })
                            .min_by(|(_, a), (_, b)| a.pivot[2].total_cmp(&b.pivot[2]))
                            .map(|(index, _)| index)
                    });
                if let Some(index) = preferred {
                    let part = &mut binding.parts[index];
                    part.fire_intensity = part.fire_intensity.max(0.78);
                    part.fire_remaining_seconds = part.fire_remaining_seconds.max(12.0);
                    part.presentation_override = true;
                }
            }
            VehicleEventKind::PetrolFireStarted => {
                let mut candidates = binding
                    .parts
                    .iter()
                    .enumerate()
                    .filter(|(_, part)| {
                        part.visible
                            && part.detached_entity.is_none()
                            && !matches!(
                                part.role,
                                ModelFragmentPartRole::Glass | ModelFragmentPartRole::Light
                            )
                    })
                    .map(|(index, part)| (index, part.pivot[2]))
                    .collect::<Vec<_>>();
                candidates.sort_by(|a, b| b.1.total_cmp(&a.1));
                for (index, _) in candidates.into_iter().take(2) {
                    let part = &mut binding.parts[index];
                    part.fire_intensity = part.fire_intensity.max(0.84);
                    part.fire_remaining_seconds = part.fire_remaining_seconds.max(12.0);
                    part.presentation_override = true;
                }
            }
            _ => {}
        }
    }
}
