use std::collections::{BTreeMap, BTreeSet};

use newviso_asset_interfaces::{
    AssetDependencyRecord, MapCellCoordV1, MapCellRefV1, MapCellV1, MapIndexV1, MapLayerRefV1,
    MapPlacementV1, MapTransformV1, MAP_CELL_SCHEMA_V1, MAP_INDEX_SCHEMA_V1,
};
use newviso_authored_xml as authored_xml;

#[derive(Clone, Debug)]
pub(crate) struct ParsedMapV1 {
    pub index: MapIndexV1,
    pub cells: BTreeMap<MapCellCoordV1, MapCellV1>,
    pub dependencies: Vec<AssetDependencyRecord>,
    pub warnings: Vec<String>,
}

pub(crate) fn parse_discrete_map_xml(source: &str, body: &[u8]) -> Result<ParsedMapV1, String> {
    let text = std::str::from_utf8(body)
        .map_err(|error| format!("YMAP v2 body is not UTF-8 source='{source}' err='{error}'"))?;
    let document = authored_xml::parse_xml_document(text, &format!("ymap source='{source}'"))?;
    let root = document.root_element();
    if !authored_xml::root_has_any_name(root, &["YmapMapDefinition", "MapDefinition"]) {
        return Err(format!(
            "YMAP v2 root must be <YmapMapDefinition> source='{source}' actual='{}'",
            root.tag_name().name()
        ));
    }
    let schema = authored_xml::root_schema(root);
    if schema == "newengine.map.definition.v1" {
        return parse_legacy_v1_projection(source, root);
    }
    if schema != "newengine.map.definition.v2" {
        return Err(format!(
            "YMAP unsupported schema source='{source}' schema='{schema}' expected='newengine.map.definition.v1|newengine.map.definition.v2'"
        ));
    }

    let map = authored_xml::xml_child(root, "map")
        .ok_or_else(|| format!("YMAP v2 has no <map> source='{source}'"))?;
    let map_id = authored_xml::xml_attr_any(map, &["id", "name"])
        .ok_or_else(|| format!("YMAP v2 <map> requires id source='{source}'"))?;
    let cell_size = authored_xml::xml_attr_f32_any(map, &["cell_size", "cellSize"])
        .unwrap_or(newviso_asset_interfaces::MAP_DEFAULT_CELL_SIZE);
    let origin = parse_vec3_attr(map, &["origin"], [0.0; 3], "map origin")?;

    let mut index = MapIndexV1 {
        schema: MAP_INDEX_SCHEMA_V1.to_owned(),
        map_id,
        origin,
        cell_size,
        tags: authored_xml::xml_tags(map, "tags"),
        metadata: parse_metadata(map),
        ..Default::default()
    };

    let mut cells = BTreeMap::new();
    let cells_node = authored_xml::xml_child(map, "cells")
        .ok_or_else(|| format!("YMAP v2 <map> requires <cells> source='{source}'"))?;
    for cell_node in authored_xml::xml_children_named(cells_node, "Cell") {
        let x = parse_i32_attr(cell_node, "x")?;
        let z = parse_i32_attr(cell_node, "z")?;
        let coord = MapCellCoordV1::new(x, z);
        if cells.contains_key(&coord) {
            return Err(format!(
                "YMAP v2 duplicate cell coord={x},{z} source='{source}'"
            ));
        }
        let mut cell = MapCellV1 {
            schema: MAP_CELL_SCHEMA_V1.to_owned(),
            coord,
            tags: authored_xml::xml_tags(cell_node, "tags"),
            metadata: parse_metadata(cell_node),
            ..Default::default()
        };
        if let Some(placements) = authored_xml::xml_child(cell_node, "placements") {
            for placement_node in authored_xml::xml_children_named(placements, "Placement") {
                cell.placements
                    .push(parse_placement(placement_node, coord)?);
            }
        }
        cell.normalize();
        cell.validate().map_err(|errors| {
            format!(
                "YMAP v2 invalid cell coord={x},{z} source='{source}' errors=[{}]",
                errors.join("; ")
            )
        })?;
        index.cells.push(MapCellRefV1::canonical(coord));
        cells.insert(coord, cell);
    }

    if let Some(layers_node) = authored_xml::xml_child(map, "layers") {
        for layer_node in authored_xml::xml_children_named(layers_node, "Layer") {
            let mut layer = MapLayerRefV1 {
                id: authored_xml::xml_attr_any(layer_node, &["id", "name"]).unwrap_or_default(),
                map_ref: authored_xml::xml_attr_any(layer_node, &["map_ref", "mapRef", "ref"])
                    .unwrap_or_default(),
                mode: authored_xml::xml_attr_any(layer_node, &["mode"])
                    .unwrap_or_else(|| "additive".to_owned()),
                priority: parse_i32_attr_default(layer_node, "priority", 0)?,
                enabled: authored_xml::xml_attr_bool_any(layer_node, &["enabled"]).unwrap_or(true),
            };
            layer.normalize();
            index.layers.push(layer);
        }
    }

    index.normalize();
    index.validate().map_err(|errors| {
        format!(
            "YMAP v2 invalid map index source='{source}' errors=[{}]",
            errors.join("; ")
        )
    })?;

    let dependencies = collect_dependencies(&index, &cells);
    Ok(ParsedMapV1 {
        index,
        cells,
        dependencies,
        warnings: Vec::new(),
    })
}

fn parse_legacy_v1_projection(
    source: &str,
    root: authored_xml::XmlNode<'_, '_>,
) -> Result<ParsedMapV1, String> {
    let map = authored_xml::xml_child(root, "map")
        .ok_or_else(|| format!("YMAP v1 has no <map> source='{source}'"))?;
    let map_id = authored_xml::xml_attr_any(map, &["id", "name"])
        .ok_or_else(|| format!("YMAP v1 <map> requires id/name source='{source}'"))?;
    let cell_size = authored_xml::xml_attr_f32_any(map, &["cell_size", "cellSize"])
        .unwrap_or(newviso_asset_interfaces::MAP_DEFAULT_CELL_SIZE);
    if !cell_size.is_finite() || cell_size <= 0.0 {
        return Err(format!(
            "YMAP v1 compatibility projection requires finite cell_size > 0 source='{source}' value={cell_size}"
        ));
    }
    let origin = parse_vec3_attr(map, &["origin"], [0.0; 3], "map origin")?;

    let mut index = MapIndexV1 {
        schema: MAP_INDEX_SCHEMA_V1.to_owned(),
        map_id,
        origin,
        cell_size,
        tags: authored_xml::xml_tags(map, "tags"),
        metadata: parse_metadata(map),
        ..Default::default()
    };
    index.metadata.insert(
        "source_schema".to_owned(),
        "newengine.map.definition.v1".to_owned(),
    );
    index.metadata.insert(
        "projection".to_owned(),
        "legacy-v1-top-level-placements-only".to_owned(),
    );

    let mut cells = BTreeMap::<MapCellCoordV1, MapCellV1>::new();
    if let Some(placements) = authored_xml::xml_child(map, "placements") {
        for (ordinal, placement_node) in authored_xml::xml_children_named(placements, "Placement")
            .into_iter()
            .enumerate()
        {
            let placement = parse_legacy_v1_placement(placement_node, ordinal)?;
            let coord = world_position_to_cell(origin, cell_size, placement.transform.position)?;
            let cell = cells.entry(coord).or_insert_with(|| MapCellV1 {
                schema: MAP_CELL_SCHEMA_V1.to_owned(),
                coord,
                ..Default::default()
            });
            cell.placements.push(placement);
        }
    }

    for cell in cells.values_mut() {
        cell.normalize();
        cell.validate().map_err(|errors| {
            format!(
                "YMAP v1 compatibility projection produced invalid cell coord={},{} source='{source}' errors=[{}]",
                cell.coord.x,
                cell.coord.z,
                errors.join("; ")
            )
        })?;
        index.cells.push(MapCellRefV1::canonical(cell.coord));
    }

    index.normalize();
    index.validate().map_err(|errors| {
        format!(
            "YMAP v1 compatibility projection produced invalid map index source='{source}' errors=[{}]",
            errors.join("; ")
        )
    })?;

    let dependencies = collect_dependencies(&index, &cells);
    Ok(ParsedMapV1 {
        index,
        cells,
        dependencies,
        warnings: vec![
            "legacy YMAP v1 compatibility projection active: only top-level <map><placements> are projected into generic discrete-map DTOs; profile/gameplay/mission/player/terrain/sky extensions remain outside engine.assets.maps".to_owned(),
        ],
    })
}

fn parse_legacy_v1_placement(
    node: authored_xml::XmlNode<'_, '_>,
    ordinal: usize,
) -> Result<MapPlacementV1, String> {
    let definition_ref = authored_xml::xml_attr_any(
        node,
        &["definition_ref", "definitionRef", "definition", "ref"],
    )
    .unwrap_or_default();
    let id = authored_xml::xml_attr_any(node, &["id", "name"])
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| format!("legacy-placement-{ordinal:04}"));
    let mut placement = MapPlacementV1 {
        id,
        definition_ref,
        transform: MapTransformV1 {
            position: parse_vec3_attr(node, &["position"], [0.0; 3], "placement position")?,
            rotation_ypr: parse_vec3_attr(
                node,
                &["rotation_ypr", "rotationYpr", "rotation"],
                [0.0; 3],
                "placement rotation_ypr",
            )?,
            scale: parse_vec3_attr(node, &["scale"], [1.0; 3], "placement scale")?,
        },
        apply_mode: authored_xml::xml_attr_any(node, &["apply_mode", "applyMode"])
            .unwrap_or_else(|| "metadata_only".to_owned()),
        tags: authored_xml::xml_attr_any(node, &["tags"])
            .map(|value| split_csv(&value))
            .unwrap_or_default(),
        enabled: authored_xml::xml_attr_bool_any(node, &["enabled"]).unwrap_or(true),
    };
    placement.normalize();
    placement.validate().map_err(|error| {
        format!(
            "YMAP v1 compatibility projection invalid placement ordinal={ordinal} id='{}': {error}",
            placement.id
        )
    })?;
    Ok(placement)
}

fn world_position_to_cell(
    origin: [f32; 3],
    cell_size: f32,
    position: [f32; 3],
) -> Result<MapCellCoordV1, String> {
    let x = ((position[0] - origin[0]) / cell_size).floor();
    let z = ((position[2] - origin[2]) / cell_size).floor();
    if !x.is_finite()
        || !z.is_finite()
        || x < i32::MIN as f32
        || x > i32::MAX as f32
        || z < i32::MIN as f32
        || z > i32::MAX as f32
    {
        return Err(format!(
            "YMAP v1 compatibility projection cell coordinate out of range position={position:?} origin={origin:?} cell_size={cell_size}"
        ));
    }
    Ok(MapCellCoordV1::new(x as i32, z as i32))
}

fn parse_placement(
    node: authored_xml::XmlNode<'_, '_>,
    coord: MapCellCoordV1,
) -> Result<MapPlacementV1, String> {
    let id = authored_xml::xml_attr_any(node, &["id", "name"]).unwrap_or_default();
    let definition_ref = authored_xml::xml_attr_any(
        node,
        &["definition_ref", "definitionRef", "definition", "ref"],
    )
    .unwrap_or_default();
    let position = parse_vec3_attr(node, &["position"], [0.0; 3], "placement position")?;
    let rotation_ypr = parse_vec3_attr(
        node,
        &["rotation_ypr", "rotationYpr", "rotation"],
        [0.0; 3],
        "placement rotation_ypr",
    )?;
    let scale = parse_vec3_attr(node, &["scale"], [1.0; 3], "placement scale")?;
    let mut tags = authored_xml::xml_tags(node, "tags");
    if let Some(inline) = authored_xml::xml_attr_any(node, &["tags"]) {
        tags.extend(split_csv(&inline));
    }
    let mut placement = MapPlacementV1 {
        id,
        definition_ref,
        transform: MapTransformV1 {
            position,
            rotation_ypr,
            scale,
        },
        apply_mode: authored_xml::xml_attr_any(node, &["apply_mode", "applyMode"])
            .unwrap_or_else(|| "instantiate".to_owned()),
        tags,
        enabled: authored_xml::xml_attr_bool_any(node, &["enabled"]).unwrap_or(true),
    };
    placement.normalize();
    placement.validate().map_err(|error| {
        format!(
            "YMAP v2 invalid placement cell={},{}, id='{}': {error}",
            coord.x, coord.z, placement.id
        )
    })?;
    Ok(placement)
}

fn collect_dependencies(
    index: &MapIndexV1,
    cells: &BTreeMap<MapCellCoordV1, MapCellV1>,
) -> Vec<AssetDependencyRecord> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for layer in &index.layers {
        if !layer.enabled || layer.map_ref.trim().is_empty() {
            continue;
        }
        if seen.insert((layer.map_ref.clone(), "map_layer".to_owned())) {
            out.push(AssetDependencyRecord::new(
                layer.map_ref.clone(),
                "map_layer",
                newviso_asset_interfaces::ENGINE_ASSETS_MAPS_SERVICE_ID,
                true,
            ));
        }
    }
    for cell in cells.values() {
        for placement in &cell.placements {
            if !placement.enabled || placement.definition_ref.trim().is_empty() {
                continue;
            }
            if seen.insert((placement.definition_ref.clone(), "definition".to_owned())) {
                let required = !placement.apply_mode.eq_ignore_ascii_case("metadata_only");
                out.push(AssetDependencyRecord::new(
                    placement.definition_ref.clone(),
                    "definition",
                    newviso_asset_interfaces::ENGINE_ASSETS_DEFINITIONS_SERVICE_ID,
                    required,
                ));
            }
        }
    }
    out.sort_by(|left, right| {
        left.reference
            .cmp(&right.reference)
            .then_with(|| left.role.cmp(&right.role))
    });
    out
}

fn parse_metadata(node: authored_xml::XmlNode<'_, '_>) -> BTreeMap<String, String> {
    let mut metadata = BTreeMap::new();
    let Some(container) = authored_xml::xml_child(node, "metadata") else {
        return metadata;
    };
    for item in container.children().filter(|child| child.is_element()) {
        let key = authored_xml::xml_attr_any(item, &["key", "name"]).unwrap_or_default();
        if key.is_empty() {
            continue;
        }
        let value = authored_xml::xml_attr_any(item, &["value"])
            .or_else(|| item.text().map(str::trim).map(ToOwned::to_owned))
            .unwrap_or_default();
        metadata.insert(key, value);
    }
    metadata
}

fn parse_vec3_attr(
    node: authored_xml::XmlNode<'_, '_>,
    names: &[&str],
    default: [f32; 3],
    label: &str,
) -> Result<[f32; 3], String> {
    let Some(value) = authored_xml::xml_attr_any(node, names) else {
        return Ok(default);
    };
    let values = split_csv(&value);
    if values.len() != 3 {
        return Err(format!(
            "{label} must contain exactly 3 comma-separated values, got '{value}'"
        ));
    }
    let mut out = [0.0; 3];
    for (index, text) in values.iter().enumerate() {
        out[index] = text
            .parse::<f32>()
            .map_err(|error| format!("{label} invalid f32 '{text}': {error}"))?;
        if !out[index].is_finite() {
            return Err(format!("{label} contains non-finite value '{text}'"));
        }
    }
    Ok(out)
}

fn split_csv(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn parse_i32_attr(node: authored_xml::XmlNode<'_, '_>, name: &str) -> Result<i32, String> {
    let value = authored_xml::xml_attr_any(node, &[name]).ok_or_else(|| {
        format!(
            "YMAP v2 node <{}> requires attribute '{name}'",
            node.tag_name().name()
        )
    })?;
    value
        .parse::<i32>()
        .map_err(|error| format!("YMAP v2 invalid {name}='{value}': {error}"))
}

fn parse_i32_attr_default(
    node: authored_xml::XmlNode<'_, '_>,
    name: &str,
    default: i32,
) -> Result<i32, String> {
    let Some(value) = authored_xml::xml_attr_any(node, &[name]) else {
        return Ok(default);
    };
    value
        .parse::<i32>()
        .map_err(|error| format!("YMAP v2 invalid {name}='{value}': {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"
<YmapMapDefinition schema="newengine.map.definition.v2" representation="xml">
  <map id="test_world" cell_size="64" origin="-64,0,-64">
    <layers>
      <Layer id="dlc" map_ref="maps/dlc.ymap@map" mode="additive" priority="100" />
    </layers>
    <cells>
      <Cell x="0" z="0">
        <placements>
          <Placement id="lamp" definition_ref="definitions/city.ytyp@lamp" position="1,2,3" />
        </placements>
      </Cell>
      <Cell x="1" z="0">
        <placements>
          <Placement id="bench" definition_ref="definitions/city.ytyp@bench" position="70,0,5" />
        </placements>
      </Cell>
    </cells>
  </map>
</YmapMapDefinition>
"#;

    #[test]
    fn parses_index_and_cells() {
        let parsed = parse_discrete_map_xml("maps/test.ymap", FIXTURE.as_bytes()).unwrap();
        assert_eq!(parsed.index.map_id, "test_world");
        assert_eq!(parsed.index.cells.len(), 2);
        assert_eq!(parsed.cells.len(), 2);
        assert_eq!(
            parsed.index.cells[0].entry,
            MapCellCoordV1::new(0, 0).canonical_entry()
        );
        assert_eq!(parsed.dependencies.len(), 3);
    }

    #[test]
    fn projects_legacy_v1_top_level_placements_only() {
        let body = br#"
<YmapMapDefinition schema="newengine.map.definition.v1">
  <map name="legacy_forest">
    <placements>
      <Placement definition_ref="definitions/world.ytyp@sky" position="0,0,0" apply_mode="metadata_only" />
    </placements>
    <profile>
      <definitions>
        <Definition definition_ref="definitions/gameplay.ytyp@player" position="128,0,128" apply_mode="instantiate" />
      </definitions>
      <prefabs><Prefab source="models/ignored.ydd@ignored" /></prefabs>
    </profile>
  </map>
</YmapMapDefinition>
"#;
        let parsed = parse_discrete_map_xml("maps/legacy.ymap", body).unwrap();
        assert_eq!(parsed.index.map_id, "legacy_forest");
        assert_eq!(parsed.cells.len(), 1);
        let cell = parsed.cells.values().next().unwrap();
        assert_eq!(cell.placements.len(), 1);
        assert_eq!(cell.placements[0].id, "legacy-placement-0000");
        assert_eq!(cell.placements[0].apply_mode, "metadata_only");
        assert_eq!(
            cell.placements[0].definition_ref,
            "definitions/world.ytyp@sky"
        );
        assert_eq!(parsed.dependencies.len(), 1);
        assert!(!parsed.dependencies[0].required);
        assert!(parsed.warnings[0].contains("top-level <map><placements>"));
    }

    #[test]
    fn legacy_v1_projection_groups_placements_into_discrete_cells() {
        let body = br#"
<YmapMapDefinition schema="newengine.map.definition.v1">
  <map name="legacy_cells" cell_size="64">
    <placements>
      <Placement id="a" definition_ref="definitions/world.ytyp@a" position="1,0,1" apply_mode="instantiate" />
      <Placement id="b" definition_ref="definitions/world.ytyp@b" position="70,0,1" apply_mode="metadata_only" />
    </placements>
  </map>
</YmapMapDefinition>
"#;
        let parsed = parse_discrete_map_xml("maps/legacy_cells.ymap", body).unwrap();
        assert_eq!(parsed.cells.len(), 2);
        assert!(parsed.cells.contains_key(&MapCellCoordV1::new(0, 0)));
        assert!(parsed.cells.contains_key(&MapCellCoordV1::new(1, 0)));
        let required = parsed
            .dependencies
            .iter()
            .map(|dependency| (dependency.reference.as_str(), dependency.required))
            .collect::<BTreeMap<_, _>>();
        assert!(required["definitions/world.ytyp@a"]);
        assert!(!required["definitions/world.ytyp@b"]);
    }

    #[test]
    fn legacy_v1_projection_rejects_direct_model_placement() {
        let body = br#"
<YmapMapDefinition schema="newengine.map.definition.v1">
  <map name="invalid"><placements>
    <Placement definition_ref="models/tree.ydd@tree" position="0,0,0" />
  </placements></map>
</YmapMapDefinition>
"#;
        let error = parse_discrete_map_xml("maps/invalid.ymap", body).unwrap_err();
        assert!(error.contains("expected .ytyp path"));
    }
}
