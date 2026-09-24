use super::*;

impl EngineApplication {
    pub(super) fn scene_stream_owner(stable_id: u64) -> StreamingOwnerId {
        StreamingOwnerId::from_label(&format!("newviso.scene.entity.{stable_id}"))
    }
    pub(super) fn sync_scene_streaming_interests(&mut self) -> Result<(), String> {
        let interests = self.scene.streaming_interests();
        let active_ids = interests
            .iter()
            .map(|request| request.stable_id)
            .collect::<Vec<_>>();

        let stale_ids = self
            .scene_stream_claims
            .keys()
            .copied()
            .filter(|stable_id| !active_ids.contains(stable_id))
            .collect::<Vec<_>>();

        for stable_id in stale_ids {
            if let Some(address) = self.scene_stream_claims.remove(&stable_id) {
                self.asset_streamer
                    .release(Self::scene_stream_owner(stable_id), &address);
                self.scene.mark_entity_unloaded(stable_id)?;
            }
        }

        for request in interests {
            let address = AssetAddress::parse(&request.asset_ref).map_err(|error| {
                format!(
                    "scene entity {} has invalid asset_ref '{}': {error}",
                    request.stable_id, request.asset_ref
                )
            })?;

            if let Some(previous) = self.scene_stream_claims.get(&request.stable_id).cloned() {
                if previous != address {
                    self.asset_streamer
                        .release(Self::scene_stream_owner(request.stable_id), &previous);
                    self.scene.mark_entity_unloaded(request.stable_id)?;
                }
            }

            self.asset_streamer.request(
                Self::scene_stream_owner(request.stable_id),
                address.clone(),
                StreamingClaim::new(request.priority.max(0.0)),
            )?;
            self.scene_stream_claims.insert(request.stable_id, address);
        }

        Ok(())
    }
    pub(super) fn apply_scene_streaming_residency(&mut self) -> Result<(), String> {
        let claims = self
            .scene_stream_claims
            .iter()
            .map(|(stable_id, address)| (*stable_id, address.clone()))
            .collect::<Vec<_>>();

        for (stable_id, address) in claims {
            if self.asset_streamer.is_resident(&address) {
                self.scene.mark_entity_resident(stable_id)?;
            }
        }
        Ok(())
    }
    pub(super) fn pump_asset_streaming(&mut self) -> Result<(), String> {
        let report = self.asset_streamer.pump();
        for (address, error) in &report.failed {
            host::warn(
                "newviso.assets.streaming",
                format!("asset='{}' streaming failed: {error}", address.canonical()),
            );
        }

        self.apply_scene_streaming_residency()?;

        if !report.loaded.is_empty()
            || !report.became_resident.is_empty()
            || !report.evicted.is_empty()
        {
            host::debug(
                "newviso.assets.streaming",
                format!(
                    "frame={} loaded={} resident_promotions={} evicted={} source_bytes_loaded={} resident_bytes={} over_budget={}",
                    report.frame,
                    report.loaded.len(),
                    report.became_resident.len(),
                    report.evicted.len(),
                    report.source_bytes_loaded,
                    report.resident_bytes,
                    report.over_budget
                ),
            );
        }
        Ok(())
    }
}
