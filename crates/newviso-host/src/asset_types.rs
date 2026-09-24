use super::*;

const ASSET_TYPES_SERVICE_ID: &str = "asset.types.api";
const ENGINE_ASSET_TYPES_SERVICE_ID: &str = "engine.assets.types";
const ASSET_TYPES_REGISTER_METHOD: &str = "asset.types.register_json_v1";
const ASSET_TYPES_MANIFEST_METHOD: &str = "asset.types.manifest_json_v1";
const ASSET_TYPES_PROBE_METHOD: &str = "asset.types.probe_json_v1";
const ASSET_TYPES_RESOLVE_METHOD: &str = "asset.types.resolve_json_v1";

#[derive(Default)]
struct AssetTypesService {
    formats: RwLock<BTreeMap<String, serde_json::Value>>,
}

impl AssetTypesService {
    fn extension_from_path(path: &str) -> String {
        path.split('@')
            .next()
            .unwrap_or(path)
            .rsplit_once('.')
            .map(|(_, ext)| ext.trim().to_ascii_lowercase())
            .unwrap_or_default()
    }

    fn encode(value: &serde_json::Value) -> RResult<Blob, RString> {
        match serde_json::to_vec(value) {
            Ok(bytes) => RResult::ROk(Blob::from(bytes)),
            Err(error) => RResult::RErr(RString::from(error.to_string())),
        }
    }
}

impl ServiceV1 for AssetTypesService {
    fn id(&self) -> CapabilityId {
        CapabilityId::from(ASSET_TYPES_SERVICE_ID)
    }

    fn describe(&self) -> RString {
        RString::from(
            serde_json::json!({
                "service": ASSET_TYPES_SERVICE_ID,
                "engine_gateway": ENGINE_ASSET_TYPES_SERVICE_ID,
                "ownership": "host",
                "policy": "starts empty; AssetManager codec DLLs self-register descriptors"
            })
            .to_string(),
        )
    }

    fn call(&self, method: MethodName, payload: Blob) -> RResult<Blob, RString> {
        match method.as_str() {
            "info_json" => {
                let count = self
                    .formats
                    .read()
                    .expect("asset type registry poisoned")
                    .len();
                Self::encode(&serde_json::json!({
                    "service": ASSET_TYPES_SERVICE_ID,
                    "engine_gateway": ENGINE_ASSET_TYPES_SERVICE_ID,
                    "registered_formats": count
                }))
            }
            ASSET_TYPES_REGISTER_METHOD => {
                let request: serde_json::Value = match serde_json::from_slice(payload.as_slice()) {
                    Ok(value) => value,
                    Err(error) => {
                        return RResult::RErr(RString::from(format!(
                            "invalid asset type registration JSON: {error}"
                        )))
                    }
                };
                let Some(descriptor) = request.get("descriptor").cloned() else {
                    return RResult::RErr(RString::from(
                        "asset type registration is missing descriptor",
                    ));
                };
                let extension = descriptor
                    .get("extension")
                    .and_then(serde_json::Value::as_str)
                    .map(|value| value.trim().trim_start_matches('.').to_ascii_lowercase())
                    .unwrap_or_default();
                if extension.is_empty() {
                    return RResult::RErr(RString::from(
                        "asset type descriptor extension is empty",
                    ));
                }

                let priority = descriptor
                    .get("priority")
                    .and_then(serde_json::Value::as_i64)
                    .unwrap_or(0);
                let mut formats = self.formats.write().expect("asset type registry poisoned");
                let should_replace = formats
                    .get(&extension)
                    .and_then(|current| current.get("priority"))
                    .and_then(serde_json::Value::as_i64)
                    .map(|current| priority >= current)
                    .unwrap_or(true);
                if should_replace {
                    formats.insert(extension, descriptor.clone());
                }
                Self::encode(&descriptor)
            }
            ASSET_TYPES_MANIFEST_METHOD => {
                let formats = self.formats.read().expect("asset type registry poisoned");
                let descriptors = formats.values().cloned().collect::<Vec<_>>();
                Self::encode(&serde_json::json!({
                    "schema": "newengine.asset_types.v2",
                    "gateway": ENGINE_ASSET_TYPES_SERVICE_ID,
                    "formats": descriptors
                }))
            }
            ASSET_TYPES_PROBE_METHOD | ASSET_TYPES_RESOLVE_METHOD => {
                let request: serde_json::Value = match serde_json::from_slice(payload.as_slice()) {
                    Ok(value) => value,
                    Err(error) => {
                        return RResult::RErr(RString::from(format!(
                            "invalid asset type probe JSON: {error}"
                        )))
                    }
                };
                let logical_path = request
                    .get("logical_path")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default();
                let extension = Self::extension_from_path(logical_path);
                let descriptor = self
                    .formats
                    .read()
                    .expect("asset type registry poisoned")
                    .get(&extension)
                    .cloned();
                Self::encode(&serde_json::json!({
                    "logical_path": logical_path,
                    "extension": extension,
                    "known": descriptor.is_some(),
                    "descriptor": descriptor
                }))
            }
            "shutdown_v1" => RResult::ROk(Blob::new()),
            other => RResult::RErr(RString::from(format!(
                "unknown asset type registry method: {other}"
            ))),
        }
    }
}

pub fn ensure_asset_types_registry() -> Result<(), String> {
    {
        let host = state().read().expect("NewViso host state poisoned");
        if host.services.contains_key(ASSET_TYPES_SERVICE_ID) {
            return Ok(());
        }
    }

    let service: ServiceV1Dyn<'static> =
        ServiceV1_TO::from_value(AssetTypesService::default(), TD_Opaque);
    match register_service_v1(service) {
        RResult::ROk(()) => Ok(()),
        RResult::RErr(error) => Err(error.to_string()),
    }
}
