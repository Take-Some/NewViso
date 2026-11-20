#![forbid(unsafe_op_in_unsafe_fn)]
//! Discrete-map semantic service for the active NewViso ABI.
//! The service reads assets through engine.assets; it cannot mutate ECS/physics.
mod parsing;
mod maps_core;
use maps_core::{map_index,map_cell_v1,map_cell_v2,map_dependencies,map_validation};
use parsing::{parse_discrete_map_xml,ParsedMapV1};
use std::{collections::BTreeMap,sync::{Arc,Mutex}};
use abi_stable::{sabi_trait::TD_Opaque,std_types::{RResult,RString}};
use newviso_plugin_abi::{
    HostApiV1,ServiceV1,ServiceV1Dyn,ServiceV1_TO,
    CapabilityId,MethodName,Blob
};
use newviso_asset_interfaces::{
    maps_method,require_asset_reference_extension,AssetDecodeRequest,
    MapCellRequestV1,MapDependenciesV1,MapIndexV1,MapRefRequestV1,
    MapResolvedCellV1,MapResolvedCellV2,MapValidationV1,
    ASSET_LIST_FILE_BODY_OUTPUT,ENGINE_ASSETS_MAPS_SERVICE_ID,
    MAPS_RUNTIME_CONTRACT,MAPS_SERVICE_ID,MAPS_SERVICE_METHODS,MAP_INDEX_ENTRY
};

#[derive(Clone)]
pub struct HostAssetClient{host:HostApiV1}
impl HostAssetClient{
    pub fn new(host:HostApiV1)->Self{Self{host}}
    fn call(&self,method:&str,payload:&[u8])->Result<Vec<u8>,String>{
        (self.host.call_service_v1)(
            CapabilityId::from("engine.assets"),
            MethodName::from(method),
            Blob::from(payload.to_vec())
        ).into_result()
         .map(|result|result.into_iter().collect())
         .map_err(|error|error.to_string())
    }
    pub fn raw_bytes_v1(&self,path:&str)->Result<Vec<u8>,String>{
        self.call("asset.raw_bytes_v1",path.as_bytes())
    }
    pub fn decode_v1(&self,request:&AssetDecodeRequest)->Result<Vec<u8>,String>{
        let payload=serde_json::to_vec(request).map_err(|e|e.to_string())?;
        self.call("asset.decode_v1",&payload)
    }
}

#[derive(Clone)]
pub struct MapsRuntimeState{
    client:HostAssetClient,
    parsed_cache:BTreeMap<String,Arc<ParsedMapV1>>
}
impl MapsRuntimeState{
    pub fn new(client:HostAssetClient)->Self{
        Self{client,parsed_cache:BTreeMap::new()}
    }
}

fn response<T:serde::Serialize>(value:Result<T,String>)->RResult<Blob,RString>{
    match value.and_then(|v|serde_json::to_vec(&v).map_err(|e|e.to_string())){
        Ok(bytes)=>RResult::ROk(Blob::from(bytes)),
        Err(error)=>RResult::RErr(RString::from(error))
    }
}
fn request<T:serde::de::DeserializeOwned>(data:&Blob)->Result<T,String>{
    serde_json::from_slice(data.as_slice()).map_err(|e|e.to_string())
}

pub struct MapsService{
    state:Mutex<MapsRuntimeState>,
    description:RString
}
impl ServiceV1 for MapsService{
    fn id(&self)->CapabilityId{CapabilityId::from(MAPS_SERVICE_ID)}
    fn describe(&self)->RString{self.description.clone()}
    fn call(&self,method:MethodName,payload:Blob)->RResult<Blob,RString>{
        let mut state=match self.state.lock(){
            Ok(state)=>state,
            Err(_)=>return RResult::RErr(RString::from("maps state poisoned"))
        };
        // Maintain all original discrete-map API method names and DTOs.
        match method.as_str(){
            maps_method::INDEX_V1=>response(request::<MapRefRequestV1>(&payload).and_then(|r|map_index(&mut state,r))),
            maps_method::CELL_V1=>response(request::<MapCellRequestV1>(&payload).and_then(|r|map_cell_v1(&mut state,r))),
            maps_method::CELL_V2=>response(request::<MapCellRequestV1>(&payload).and_then(|r|map_cell_v2(&mut state,r))),
            maps_method::VALIDATE_V1=>response(request::<MapRefRequestV1>(&payload).map(|r|map_validation(&mut state,r))),
            maps_method::DEPENDENCIES_V1=>response(request::<MapRefRequestV1>(&payload).and_then(|r|map_dependencies(&mut state,r))),
            maps_method::INVOKE_JSON=>{
                let parsed=serde_json::from_slice::<serde_json::Value>(payload.as_slice());
                let value=match parsed{
                    Ok(v)=>v,Err(e)=>return RResult::RErr(RString::from(e.to_string()))
                };
                let method=value.get("method").and_then(|v|v.as_str()).unwrap_or(maps_method::VALIDATE_V1);
                let body=serde_json::to_vec(&value.get("request").cloned().unwrap_or_default()).unwrap_or_default();
                let req=Blob::from(body);
                match method{
                    maps_method::INDEX_V1=>response(request::<MapRefRequestV1>(&req).and_then(|r|map_index(&mut state,r))),
                    maps_method::CELL_V1=>response(request::<MapCellRequestV1>(&req).and_then(|r|map_cell_v1(&mut state,r))),
                    maps_method::CELL_V2=>response(request::<MapCellRequestV1>(&req).and_then(|r|map_cell_v2(&mut state,r))),
                    maps_method::VALIDATE_V1=>response(request::<MapRefRequestV1>(&req).map(|r|map_validation(&mut state,r))),
                    maps_method::DEPENDENCIES_V1=>response(request::<MapRefRequestV1>(&req).and_then(|r|map_dependencies(&mut state,r))),
                    _=>RResult::RErr(RString::from(format!("unknown maps invocation {method}")))
                }
            }
            maps_method::SHUTDOWN_V1=>RResult::ROk(Blob::new()),
            _=>RResult::RErr(RString::from(format!("unknown maps service method {method}")))
        }
    }
}
pub fn maps_gateway_service(host:HostApiV1)->ServiceV1Dyn<'static>{
    let description=serde_json::json!({
        "id":MAPS_SERVICE_ID,
        "gateway":ENGINE_ASSETS_MAPS_SERVICE_ID,
        "contract":MAPS_RUNTIME_CONTRACT,
        "owner":"newviso-maps-runtime",
        "methods":MAPS_SERVICE_METHODS.iter().map(|m|serde_json::json!({"name":m})).collect::<Vec<_>>()
    });
    ServiceV1_TO::from_value(
        MapsService{
            state:Mutex::new(MapsRuntimeState::new(HostAssetClient::new(host))),
            description:RString::from(description.to_string())
        },
        TD_Opaque
    )
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn discrete_map_api_declares_cell_versions(){
        assert!(MAPS_SERVICE_METHODS.contains(&maps_method::CELL_V1));
        assert!(MAPS_SERVICE_METHODS.contains(&maps_method::CELL_V2));
        assert!(MAPS_SERVICE_METHODS.contains(&maps_method::INDEX_V1));
    }
}
