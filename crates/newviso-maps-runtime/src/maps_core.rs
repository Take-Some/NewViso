use super::*;
fn canonical_map_source(request: &MapRefRequestV1) -> Result<(String, String), String> {
    let reference = require_asset_reference_extension(&request.map_ref, &["ymap"], false)?;
    if let Some(entry) = reference.entry.as_deref() {
        if !entry.eq_ignore_ascii_case(MAP_INDEX_ENTRY) {
            return Err(format!(
                "engine.assets.maps index requires .ymap@map or bare .ymap, got '{}'",
                reference.canonical
            ));
        }
    }
    let canonical = format!("{}@{}", reference.logical_path, MAP_INDEX_ENTRY);
    Ok((reference.logical_path, canonical))
}

fn load_map_body(state: &MapsRuntimeState, source: &str) -> Result<(Vec<u8>, Vec<String>), String> {
    match state.client.raw_bytes_v1(source) {
        Ok(body) if newviso_authored_xml::body_is_xml(&body) => Ok((
            body,
            vec![".ymap loose XML authoring body read through engine.assets raw_bytes_v1".to_owned()],
        )),
        Ok(_encoded_or_source_dictionary) => decode_map_body(state, source).map(|body| {
            (
                body,
                vec![".ymap encoded/source body decoded through engine.assets".to_owned()],
            )
        }),
        Err(read_error) => decode_map_body(state, source)
            .map(|body| {
                (
                    body,
                    vec![".ymap body decoded through engine.assets after raw_bytes_v1 miss".to_owned()],
                )
            })
            .map_err(|decode_error| {
                format!(
                    "engine.assets.maps: .ymap unavailable source='{source}' read_err='{read_error}' decode_err='{decode_error}'"
                )
            }),
    }
}

fn decode_map_body(state: &MapsRuntimeState, source: &str) -> Result<Vec<u8>, String> {
    state
        .client
        .decode_v1(&AssetDecodeRequest {
            logical_path: source.to_owned(),
            output_kind: ASSET_LIST_FILE_BODY_OUTPUT.to_owned(),
            selector: serde_json::Value::Null,
                    format_descriptor: None,
})
        .map_err(|error| {
            format!(
                "engine.assets.maps: asset.decode_v1 failed source='{source}' output='{ASSET_LIST_FILE_BODY_OUTPUT}' err='{error}'"
            )
        })
}

fn load_parsed_map(
    state: &mut MapsRuntimeState,
    request: &MapRefRequestV1,
) -> Result<(String, Arc<ParsedMapV1>), String> {
    let (source, canonical) = canonical_map_source(request)?;
    if let Some(parsed) = state.parsed_cache.get(&source) {
        return Ok((canonical, Arc::clone(parsed)));
    }
    let (body, mut transport_warnings) = load_map_body(state, &source)?;
    if !newviso_authored_xml::body_is_xml(&body) {
        return Err(format!(
            "engine.assets.maps: YMAP semantic body must currently be XML source='{source}' bytes={} schemas='newengine.map.definition.v1|newengine.map.definition.v2'",
            body.len()
        ));
    }
    let mut parsed = parse_discrete_map_xml(&source, &body)?;
    transport_warnings.append(&mut parsed.warnings);
    parsed.warnings = transport_warnings;
    let parsed = Arc::new(parsed);
    state.parsed_cache.insert(source, Arc::clone(&parsed));
    Ok((canonical, parsed))
}

pub(super) fn map_index(state: &mut MapsRuntimeState, request: MapRefRequestV1) -> Result<MapIndexV1, String> {
    load_parsed_map(state, &request).map(|(_, parsed)| parsed.index.clone())
}

fn resolve_map_cell(
    state: &mut MapsRuntimeState,
    request: MapCellRequestV1,
) -> Result<
    (
        String,
        String,
        Arc<ParsedMapV1>,
        newviso_asset_interfaces::MapCellV1,
    ),
    String,
> {
    let map_request = MapRefRequestV1 {
        map_ref: request.map_ref,
    };
    let (canonical_map_ref, parsed) = load_parsed_map(state, &map_request)?;
    let cell = parsed.cells.get(&request.coord).cloned().ok_or_else(|| {
        format!(
            "engine.assets.maps: cell not declared map='{}' coord={},{}",
            canonical_map_ref, request.coord.x, request.coord.z
        )
    })?;
    let cell_ref = format!(
        "{}@{}",
        canonical_map_ref.split('@').next().unwrap_or_default(),
        request.coord.canonical_entry()
    );
    Ok((canonical_map_ref, cell_ref, parsed, cell))
}

pub(super) fn map_cell_v1(
    state: &mut MapsRuntimeState,
    request: MapCellRequestV1,
) -> Result<MapResolvedCellV1, String> {
    let (map_ref, cell_ref, parsed, cell) = resolve_map_cell(state, request)?;
    Ok(MapResolvedCellV1 {
        map_ref,
        cell_ref,
        index: parsed.index.clone(),
        cell,
    })
}

pub(super) fn map_cell_v2(
    state: &mut MapsRuntimeState,
    request: MapCellRequestV1,
) -> Result<MapResolvedCellV2, String> {
    let (map_ref, cell_ref, _parsed, cell) = resolve_map_cell(state, request)?;
    Ok(MapResolvedCellV2 {
        map_ref,
        cell_ref,
        cell,
    })
}

pub(super) fn map_dependencies(
    state: &mut MapsRuntimeState,
    request: MapRefRequestV1,
) -> Result<MapDependenciesV1, String> {
    let (canonical_map_ref, parsed) = load_parsed_map(state, &request)?;
    Ok(MapDependenciesV1 {
        map_ref: canonical_map_ref,
        dependencies: parsed.dependencies.clone(),
    })
}

pub(super) fn map_validation(state: &mut MapsRuntimeState, request: MapRefRequestV1) -> MapValidationV1 {
    let requested_ref = request.map_ref.clone();
    match load_parsed_map(state, &request) {
        Ok((canonical_map_ref, parsed)) => MapValidationV1 {
            ok: true,
            map_ref: canonical_map_ref,
            cell_count: u32::try_from(parsed.cells.len()).unwrap_or(u32::MAX),
            placement_count: parsed
                .cells
                .values()
                .map(|cell| u32::try_from(cell.placements.len()).unwrap_or(u32::MAX))
                .fold(0_u32, u32::saturating_add),
            errors: Vec::new(),
            warnings: parsed.warnings.clone(),
        },
        Err(error) => MapValidationV1 {
            ok: false,
            map_ref: requested_ref,
            errors: vec![error],
            ..Default::default()
        },
    }
}

