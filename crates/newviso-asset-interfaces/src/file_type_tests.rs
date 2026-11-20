use super::*;

#[test]
fn descriptor_normalization_does_not_infer_semantic_owner() {
    let mut descriptor = AssetFileTypeDescriptor {
        extension: "whatever".to_owned(),
        asset_kind: "opaque_format".to_owned(),
        codec_type: codec_type::LIST_FILE.to_owned(),
        handler_service: "asset.codec.listfile.whatever".to_owned(),
        magic: Some("4e454638".to_owned()),
        ..Default::default()
    };
    descriptor.normalize_layer_contract();
    assert!(descriptor.semantic_gateway.is_empty());
    assert!(descriptor.validate_generic_rules().is_err());
}

#[test]
fn explicit_descriptor_is_valid_without_registry_extension_knowledge() {
    let mut descriptor = AssetFileTypeDescriptor {
        module_id: "test.formats.opaque".to_owned(),
        family: "test".to_owned(),
        extension: "opaque".to_owned(),
        asset_kind: "provider_declared_asset".to_owned(),
        container: "newengine.listfile.nef8.opaque".to_owned(),
        content_kind: Some(9001),
        codec_type: codec_type::LIST_FILE.to_owned(),
        byte_owner: ENGINE_ASSET_SERVICE_ID.to_owned(),
        semantic_gateway: "engine.assets.provider_declared".to_owned(),
        handler_service: "asset.codec.listfile.opaque".to_owned(),
        selector_syntax: Some("file.opaque@entry".to_owned()),
        consumer_domains: vec!["engine.assets.provider_declared".to_owned()],
        magic: Some("4e454638".to_owned()),
        outputs: vec![
            ASSET_LIST_FILE_MANIFEST_OUTPUT.to_owned(),
            ASSET_LIST_FILE_BODY_OUTPUT.to_owned(),
        ],
        runtime_ready: true,
        native_container: true,
        requires_magic: true,
        ..Default::default()
    };
    descriptor.normalize_layer_contract();
    assert_eq!(descriptor.gateway, descriptor.semantic_gateway);
    assert_eq!(descriptor.content_kind, Some(9001));
    assert!(descriptor.validate_generic_rules().is_ok());
}
