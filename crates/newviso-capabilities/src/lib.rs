use newviso_provider_runtime::{probe_lifecycle, BootstrapPhase, ProviderInfo, RootSymbol};
use serde_json::Value;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct CapabilityNeed {
    pub id: String,
    pub min_version: u32,
    pub provider: Option<String>,
    pub required: bool,
}

#[derive(Clone, Debug)]
pub struct CapabilityCandidate {
    pub capability_id: String,
    pub version: u32,
    pub provider_id: String,
    pub provider_path: PathBuf,
    pub bootstrap_phase: BootstrapPhase,
    pub engine_gateway: Option<String>,
    pub service_id: Option<String>,
    pub priority: i64,
    pub describe_json: String,
}

#[derive(Clone, Debug)]
pub struct ResolvedCapability {
    pub need: CapabilityNeed,
    pub candidate: Option<CapabilityCandidate>,
}

pub fn resolve_capabilities(
    providers: &[ProviderInfo],
    needs: impl IntoIterator<Item = CapabilityNeed>,
) -> Result<Vec<ResolvedCapability>, String> {
    let catalog = build_catalog(providers)?;
    let mut resolved = Vec::new();

    for need in needs {
        let mut candidates = catalog
            .iter()
            .filter(|candidate| candidate.capability_id == need.id)
            .filter(|candidate| candidate.version >= need.min_version)
            .filter(|candidate| {
                need.provider
                    .as_deref()
                    .map(|provider| provider == candidate.provider_id)
                    .unwrap_or(true)
            })
            .cloned()
            .collect::<Vec<_>>();

        candidates.sort_by(|a, b| {
            b.priority
                .cmp(&a.priority)
                .then_with(|| a.provider_id.cmp(&b.provider_id))
        });

        let candidate = candidates.into_iter().next();
        if need.required && candidate.is_none() {
            return Err(format!(
                "required capability '{}' version>={}{} has no compatible provider",
                need.id,
                need.min_version,
                need.provider
                    .as_deref()
                    .map(|provider| format!(" provider='{provider}'"))
                    .unwrap_or_default()
            ));
        }

        resolved.push(ResolvedCapability { need, candidate });
    }

    Ok(resolved)
}

pub fn build_catalog(providers: &[ProviderInfo]) -> Result<Vec<CapabilityCandidate>, String> {
    let mut catalog = Vec::new();

    for provider in providers {
        if provider.root_symbol != RootSymbol::Current {
            continue;
        }

        let lifecycle = probe_lifecycle(&provider.path).map_err(|error| {
            format!(
                "failed to inspect capabilities for provider '{}': {error}",
                provider.id
            )
        })?;

        for capability in lifecycle.capabilities {
            if !matches!(
                capability.role,
                newviso_provider_runtime::CapabilityRole::Provides
            ) {
                continue;
            }

            let metadata = serde_json::from_str::<Value>(&capability.describe_json).ok();
            catalog.push(CapabilityCandidate {
                capability_id: capability.id,
                version: capability.version,
                provider_id: provider.id.clone(),
                provider_path: provider.path.clone(),
                bootstrap_phase: provider.bootstrap_phase,
                engine_gateway: metadata.as_ref().and_then(extract_engine_gateway),
                service_id: metadata.as_ref().and_then(extract_service_id),
                priority: metadata.as_ref().and_then(extract_priority).unwrap_or(0),
                describe_json: capability.describe_json,
            });
        }
    }

    Ok(catalog)
}

fn extract_engine_gateway(value: &Value) -> Option<String> {
    value
        .get("engine_gateway")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
}

fn extract_service_id(value: &Value) -> Option<String> {
    for key in ["contract", "service", "service_id"] {
        if let Some(value) = value.get(key).and_then(Value::as_str) {
            if !value.trim().is_empty() {
                return Some(value.to_owned());
            }
        }
    }
    value
        .get("route")
        .and_then(|route| route.get("contract"))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn extract_priority(value: &Value) -> Option<i64> {
    value
        .get("backend_priority")
        .and_then(Value::as_i64)
        .or_else(|| value.get("priority").and_then(Value::as_i64))
        .or_else(|| {
            value
                .get("route")
                .and_then(|route| route.get("backend_priority"))
                .and_then(Value::as_i64)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_route_metadata() {
        let value = serde_json::json!({
            "engine_gateway": "engine.ui",
            "contract": "aurelia.ui.api",
            "backend_priority": 250
        });
        assert_eq!(extract_engine_gateway(&value).as_deref(), Some("engine.ui"));
        assert_eq!(
            extract_service_id(&value).as_deref(),
            Some("aurelia.ui.api")
        );
        assert_eq!(extract_priority(&value), Some(250));
    }
}
