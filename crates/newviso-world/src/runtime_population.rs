use super::*;

impl LivingWorldRuntime {
    pub fn upsert_population_channel(&mut self, desc: PopulationChannelDesc) -> Result<(), String> {
        validate_id("population channel", &desc.id)?;
        if !desc.density.is_finite()
            || !(0.0..=64.0).contains(&desc.density)
            || desc.max_active > 1_000_000
            || !desc.spawn_radius.is_finite()
            || desc.spawn_radius < 0.0
            || !desc.despawn_radius.is_finite()
            || desc.despawn_radius < desc.spawn_radius
            || desc.creation_budget_per_tick > 1_000_000
            || desc.removal_budget_per_tick > 1_000_000
            || !desc.update_interval_seconds.is_finite()
            || !(0.0..=3600.0).contains(&desc.update_interval_seconds)
            || desc.model_set.as_deref().is_some_and(|id| !valid_label(id))
            || desc.tags.iter().any(|tag| !valid_label(tag))
            || data::invalid_float_map(&desc.parameters)
        {
            return Err("invalid generic PopulationChannelDesc parameters".to_owned());
        }
        self.population_channels.insert(desc.id.clone(), desc);
        Ok(())
    }

    pub fn remove_population_channel(&mut self, id: &str) {
        self.population_channels.remove(id.trim());
    }

    pub fn set_population_streaming_policy(
        &mut self,
        desc: PopulationStreamingPolicyDesc,
    ) -> Result<(), String> {
        if desc.max_resident_sets == 0
            || desc.max_resident_sets > 65_536
            || desc.request_budget_per_tick > 65_536
            || desc.eviction_budget_per_tick > 65_536
            || desc
                .fallback_set
                .as_deref()
                .is_some_and(|id| !valid_label(id))
        {
            return Err("invalid generic PopulationStreamingPolicyDesc parameters".to_owned());
        }
        self.population_streaming = desc;
        Ok(())
    }

    pub fn upsert_zone(&mut self, desc: LivingWorldZoneDesc) -> Result<(), String> {
        validate_id("living-world zone", &desc.id)?;
        if desc
            .min
            .iter()
            .chain(desc.max.iter())
            .any(|value| !value.is_finite())
            || desc
                .min
                .iter()
                .zip(desc.max.iter())
                .any(|(min, max)| min > max)
            || desc.tags.iter().any(|tag| !valid_label(tag))
            || data::invalid_float_map(&desc.parameters)
        {
            return Err("invalid generic LivingWorldZoneDesc parameters".to_owned());
        }
        self.zones.insert(desc.id.clone(), desc);
        Ok(())
    }

    pub fn remove_zone(&mut self, id: &str) {
        self.zones.remove(id.trim());
    }

    pub fn upsert_model_set(&mut self, desc: AmbientModelSetDesc) -> Result<(), String> {
        validate_id("ambient model set", &desc.id)?;
        if !valid_label(&desc.category)
            || desc.assets.is_empty()
            || desc.assets.len() > 4096
            || desc
                .assets
                .iter()
                .any(|asset| asset.trim().is_empty() || asset.len() > 1024)
            || (!desc.weights.is_empty() && desc.weights.len() != desc.assets.len())
            || desc
                .weights
                .iter()
                .any(|weight| !weight.is_finite() || *weight < 0.0)
            || desc.tags.iter().any(|tag| !valid_label(tag))
        {
            return Err("invalid generic AmbientModelSetDesc parameters".to_owned());
        }
        self.model_sets.insert(desc.id.clone(), desc);
        Ok(())
    }

    pub fn remove_model_set(&mut self, id: &str) {
        self.model_sets.remove(id.trim());
    }

    pub fn upsert_scenario_point(&mut self, desc: ScenarioPointDesc) -> Result<(), String> {
        validate_id("scenario point", &desc.id)?;
        if !valid_label(&desc.kind)
            || desc
                .group
                .as_deref()
                .is_some_and(|group| !valid_label(group))
            || desc.position.iter().any(|value| !value.is_finite())
            || !desc.heading_degrees.is_finite()
            || desc.heading_degrees.abs() > 1.0e6
            || !desc.radius.is_finite()
            || !(0.001..=1_000_000.0).contains(&desc.radius)
            || !desc.probability.is_finite()
            || !(0.0..=1.0).contains(&desc.probability)
            || desc
                .model_set
                .as_deref()
                .is_some_and(|model_set| !valid_label(model_set))
            || desc.tags.iter().any(|tag| !valid_label(tag))
            || data::invalid_float_map(&desc.parameters)
        {
            return Err("invalid generic ScenarioPointDesc parameters".to_owned());
        }
        self.scenario_points.insert(desc.id.clone(), desc);
        Ok(())
    }

    pub fn remove_scenario_point(&mut self, id: &str) {
        self.scenario_points.remove(id.trim());
        self.scenario_reservations
            .retain(|_, record| record.desc.scenario_point_id != id.trim());
    }

    pub fn upsert_relationship(&mut self, desc: RelationshipRuleDesc) -> Result<(), String> {
        if !valid_label(&desc.source_group)
            || !valid_label(&desc.target_group)
            || !valid_label(&desc.relation)
            || !desc.weight.is_finite()
            || desc.weight.abs() > 1.0e6
            || desc.tags.iter().any(|tag| !valid_label(tag))
        {
            return Err("invalid generic RelationshipRuleDesc parameters".to_owned());
        }
        self.relationships
            .insert((desc.source_group.clone(), desc.target_group.clone()), desc);
        Ok(())
    }

    pub fn remove_relationship(&mut self, source_group: &str, target_group: &str) {
        self.relationships.remove(&(
            source_group.trim().to_owned(),
            target_group.trim().to_owned(),
        ));
    }

    pub fn emit_stimulus(&mut self, mut desc: WorldStimulusDesc) -> Result<String, String> {
        if desc.id.trim().is_empty() {
            desc.id = format!("stimulus.{:016x}", self.next_stimulus_id);
            self.next_stimulus_id = self.next_stimulus_id.wrapping_add(1).max(1);
        }
        validate_id("world stimulus", &desc.id)?;
        if !valid_label(&desc.kind)
            || !valid_label(&desc.source)
            || desc.position.iter().any(|value| !value.is_finite())
            || !desc.radius.is_finite()
            || !(0.001..=1_000_000.0).contains(&desc.radius)
            || !desc.intensity.is_finite()
            || !(0.0..=1.0e9).contains(&desc.intensity)
            || !desc.lifetime_seconds.is_finite()
            || !(0.001..=86_400.0).contains(&desc.lifetime_seconds)
            || desc.tags.iter().any(|tag| !valid_label(tag))
            || !valid_json_payload(&desc.payload)
        {
            return Err("invalid generic WorldStimulusDesc parameters".to_owned());
        }

        let id = desc.id.clone();
        self.stimuli.insert(
            id.clone(),
            ActiveStimulus {
                expires_world_seconds: self.clock.world_seconds + f64::from(desc.lifetime_seconds),
                desc,
            },
        );
        Ok(id)
    }

    pub fn clear_stimulus(&mut self, id: &str) {
        self.stimuli.remove(id.trim());
    }

    pub fn stimulus_runtime_views(&self) -> Vec<WorldStimulusRuntimeView> {
        self.stimuli
            .values()
            .map(|active| WorldStimulusRuntimeView {
                id: active.desc.id.clone(),
                kind: active.desc.kind.clone(),
                source: active.desc.source.clone(),
                position: active.desc.position,
                radius: active.desc.radius,
                intensity: active.desc.intensity,
                remaining_seconds: (active.expires_world_seconds - self.clock.world_seconds)
                    .max(0.0) as f32,
                tags: active.desc.tags.clone(),
                payload: active.desc.payload.clone(),
            })
            .collect()
    }

    pub(super) fn zone_memberships(&self, point: [f32; 3]) -> Vec<String> {
        let mut zones = self
            .zones
            .values()
            .filter(|zone| point_in_aabb(point, zone.min, zone.max))
            .collect::<Vec<_>>();
        zones.sort_by(|a, b| {
            b.priority
                .cmp(&a.priority)
                .then_with(|| zone_volume(a).total_cmp(&zone_volume(b)))
                .then_with(|| a.id.cmp(&b.id))
        });
        zones.into_iter().map(|zone| zone.id.clone()).collect()
    }
}
