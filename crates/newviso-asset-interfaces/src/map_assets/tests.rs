    use super::*;

    fn placement(id: &str, definition_ref: &str) -> MapPlacementV1 {
        MapPlacementV1 {
            id: id.to_owned(),
            definition_ref: definition_ref.to_owned(),
            ..Default::default()
        }
    }

    #[test]
    fn canonical_cell_entry_is_stable() {
        assert_eq!(MapCellCoordV1::new(-2, 7).canonical_entry(), "cell/-2/7");
    }

    #[test]
    fn map_transform_accepts_mirrored_scale_but_rejects_singular_scale() {
        let mirrored = MapTransformV1 {
            scale: [-0.999_999_9, 0.999_999_8, 0.999_999_6],
            ..Default::default()
        };
        assert!(mirrored.validate().is_ok());

        let singular = MapTransformV1 {
            scale: [0.0, 1.0, 1.0],
            ..Default::default()
        };
        let error = singular.validate().unwrap_err();
        assert!(error.contains("non-zero"));
    }

    #[test]
    fn index_rejects_duplicate_cells() {
        let mut index = MapIndexV1 {
            map_id: "world".to_owned(),
            cells: vec![
                MapCellRefV1::canonical(MapCellCoordV1::new(0, 0)),
                MapCellRefV1::canonical(MapCellCoordV1::new(0, 0)),
            ],
            ..Default::default()
        };
        index.normalize();
        let errors = index.validate().unwrap_err();
        assert!(errors
            .iter()
            .any(|error| error.contains("duplicate map cell coordinate")));
    }

    #[test]
    fn cell_rejects_direct_model_refs() {
        let cell = MapCellV1 {
            placements: vec![placement("tower", "models/tower.ydd@tower")],
            ..Default::default()
        };
        let errors = cell.validate().unwrap_err();
        assert!(errors
            .iter()
            .any(|error| error.contains("expected .ytyp path")));
    }

    #[test]
    fn cell_accepts_definitionless_player_spawn_marker() {
        let cell = MapCellV1 {
            placements: vec![MapPlacementV1 {
                id: "player_start".to_owned(),
                definition_ref: String::new(),
                apply_mode: "player_spawn".to_owned(),
                tags: vec!["player_spawn".to_owned()],
                ..Default::default()
            }],
            ..Default::default()
        };
        assert!(cell.validate().is_ok());
    }

    #[test]
    fn cell_rejects_definitionless_instantiated_placement() {
        let cell = MapCellV1 {
            placements: vec![placement("orphan", "")],
            ..Default::default()
        };
        let errors = cell.validate().unwrap_err();
        assert!(errors
            .iter()
            .any(|error| error.contains("reference is empty")));
    }

    #[test]
    fn cell_accepts_definition_entries() {
        let cell = MapCellV1 {
            placements: vec![placement("tower", "definitions/world.ytyp@tower")],
            ..Default::default()
        };
        assert!(cell.validate().is_ok());
    }

    #[test]
    fn normalized_index_cell_lookup_is_binary_search_compatible() {
        let mut index = MapIndexV1 {
            map_id: "world".to_owned(),
            cells: vec![
                MapCellRefV1::canonical(MapCellCoordV1::new(50, 2)),
                MapCellRefV1::canonical(MapCellCoordV1::new(-10, 3)),
                MapCellRefV1::canonical(MapCellCoordV1::new(0, 0)),
            ],
            ..Default::default()
        };
        index.normalize();
        assert_eq!(
            index
                .cell(MapCellCoordV1::new(-10, 3))
                .map(|cell| cell.coord),
            Some(MapCellCoordV1::new(-10, 3))
        );
        assert!(index.cell(MapCellCoordV1::new(99, 99)).is_none());
    }

    #[test]
    fn compact_cell_v2_omits_redundant_map_index() {
        let index = MapIndexV1 {
            map_id: "large".to_owned(),
            cells: (0..128)
                .map(|x| MapCellRefV1::canonical(MapCellCoordV1::new(x, 0)))
                .collect(),
            ..Default::default()
        };
        let cell = MapCellV1 {
            coord: MapCellCoordV1::new(0, 0),
            ..Default::default()
        };
        let v1 = serde_json::to_vec(&MapResolvedCellV1 {
            map_ref: "maps/large.ymap@map".to_owned(),
            cell_ref: "maps/large.ymap@cell/0/0".to_owned(),
            index,
            cell: cell.clone(),
        })
        .unwrap();
        let v2 = serde_json::to_vec(&MapResolvedCellV2 {
            map_ref: "maps/large.ymap@map".to_owned(),
            cell_ref: "maps/large.ymap@cell/0/0".to_owned(),
            cell,
        })
        .unwrap();
        assert!(v2.len() < v1.len());
        assert!(!std::str::from_utf8(&v2).unwrap().contains("\"index\""));
    }

    #[test]
    fn world_position_maps_to_discrete_cell() {
        let index = MapIndexV1 {
            map_id: "world".to_owned(),
            origin: [-64.0, 0.0, -64.0],
            cell_size: 64.0,
            ..Default::default()
        };
        assert_eq!(
            index.world_to_cell([0.0, 100.0, 0.0]),
            Some(MapCellCoordV1::new(1, 1))
        );
    }

    #[test]
    fn normalization_makes_cell_payload_deterministic() {
        let mut cell = MapCellV1 {
            placements: vec![
                placement("b", "definitions\\world.ytyp@b"),
                placement("a", "definitions/world.ytyp@a"),
            ],
            tags: vec![
                " World ".to_owned(),
                "world".to_owned(),
                "STREAMING".to_owned(),
            ],
            ..Default::default()
        };
        cell.normalize();
        assert_eq!(cell.placements[0].id, "a");
        assert_eq!(
            cell.placements[1].definition_ref,
            "definitions/world.ytyp@b"
        );
        assert_eq!(cell.tags, vec!["streaming", "world"]);
    }
