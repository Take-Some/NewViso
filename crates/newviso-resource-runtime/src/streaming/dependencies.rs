use super::*;

impl<S: AssetSource> AssetStreamer<S> {
    pub(super) fn acquire_source(&mut self, load: &ResourceLoad) {
        let source = self
            .resident_sources
            .entry(load.source.clone())
            .or_insert(ResidentSource {
                bytes: load.source_size_bytes,
                references: 0,
            });
        source.bytes = source.bytes.max(load.source_size_bytes);
        source.references = source.references.saturating_add(1);
    }
    pub(super) fn release_source(&mut self, source: &ResolvedAssetSource) {
        let remove = if let Some(record) = self.resident_sources.get_mut(source) {
            record.references = record.references.saturating_sub(1);
            record.references == 0
        } else {
            false
        };
        if remove {
            self.resident_sources.remove(source);
        }
    }
    pub(super) fn refresh_dependency_claims(&mut self, parent: &AssetAddress) {
        let Some(entry) = self.entries.get(parent) else {
            return;
        };
        if entry.dependencies.is_empty() {
            return;
        }

        let inherited_priority = entry.effective_priority() * self.policy.dependency_priority_scale;
        let dependencies = entry.dependencies.clone();
        for dependency in dependencies {
            let child = self
                .entries
                .entry(dependency.clone())
                .or_insert_with(|| StreamingEntry::new(dependency.clone(), self.frame));
            child
                .dependency_claims
                .insert(parent.clone(), inherited_priority);
            child.last_touched_frame = self.frame;
            if child.state == StreamingState::Unloaded {
                child.state = StreamingState::Queued;
            }
            self.refresh_dependency_claims(&dependency);
        }
    }
    pub(super) fn remove_dependency_claims(
        &mut self,
        parent: &AssetAddress,
        dependencies: &[AssetAddress],
    ) {
        for dependency in dependencies {
            if let Some(child) = self.entries.get_mut(dependency) {
                child.dependency_claims.remove(parent);
                child.last_touched_frame = self.frame;
                if !child.has_claims()
                    && matches!(child.state, StreamingState::Queued | StreamingState::Failed)
                {
                    child.state = StreamingState::Unloaded;
                    child.error = None;
                    child.failure_frame = None;
                }
            }
        }
    }
    pub(super) fn dependency_reaches(&self, start: &AssetAddress, target: &AssetAddress) -> bool {
        if start == target {
            return true;
        }

        let mut visited = HashSet::new();
        let mut stack = vec![start.clone()];
        while let Some(current) = stack.pop() {
            if !visited.insert(current.clone()) {
                continue;
            }
            if &current == target {
                return true;
            }
            if let Some(entry) = self.entries.get(&current) {
                stack.extend(entry.dependencies.iter().cloned());
            }
        }
        false
    }
}
