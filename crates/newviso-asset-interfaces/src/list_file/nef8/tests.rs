    use super::*;

    fn round_trip(request: ListFileEncodeRequest<'_>, expected_class: u8) -> ListFileHeader {
        let bytes = encode_list_file(request).unwrap();
        let header = parse_list_file_header(&bytes).unwrap();
        assert_eq!(header.version, LIST_FILE_VERSION);
        assert_eq!(header.size_class, expected_class);
        assert_eq!(header.header_len as usize, 1_usize << expected_class);
        header
    }

    #[test]
    fn missing_header_metadata_uses_opaque_wire_content_kind_identity() {
        let bytes = encode_list_file(ListFileEncodeRequest {
            content_kind: 9001,
            content_schema_version: 1,
            entry_count: 0,
            additional_flags: 0,
            min_size_class: 4,
            header_metadata: &[],
            body_stored: &[1],
            body_uncompressed_len: 1,
            body_raw_hash: None,
            stable_file_id: None,
            import_settings_hash: None,
        })
        .unwrap();
        let header = parse_list_file_header(&bytes).unwrap();
        let metadata = read_header_metadata(&bytes, &header, "test.opaque").unwrap();
        assert_eq!(metadata.logical_path, "test.opaque");
        assert_eq!(metadata.content_kind, "opaque:9001");
    }

    #[test]
    fn blank_metadata_content_kind_uses_opaque_wire_identity_without_domain_inference() {
        let header_metadata = br#"{"schema":"metadata","logical_path":"","content_kind":""}"#;
        let bytes = encode_list_file(ListFileEncodeRequest {
            content_kind: 9002,
            content_schema_version: 1,
            entry_count: 0,
            additional_flags: 0,
            min_size_class: 5,
            header_metadata,
            body_stored: &[1],
            body_uncompressed_len: 1,
            body_raw_hash: None,
            stable_file_id: None,
            import_settings_hash: None,
        })
        .unwrap();
        let header = parse_list_file_header(&bytes).unwrap();
        let metadata = read_header_metadata(&bytes, &header, "test.opaque").unwrap();
        assert_eq!(metadata.logical_path, "test.opaque");
        assert_eq!(metadata.content_kind, "opaque:9002");
    }

    #[test]
    fn class_4_is_real_16_byte_minimal_header() {
        let body = [1_u8, 2, 3, 4];
        let request = ListFileEncodeRequest::compact(LIST_FILE_CONTENT_KIND_NEMAT, &body);
        let header = round_trip(request, 4);
        assert_eq!(header.body_offset, 16);
        assert_eq!(header.body_len, body.len() as u64);
        assert_eq!(header.body_uncompressed_len, 0);
        assert!(!header.has_body_raw_hash());
    }

    #[test]
    fn class_5_carries_lengths_and_implicit_metadata_range() {
        let body = [7_u8; 11];
        let metadata = br#"{"schema":"metadata"}"#;
        let header = round_trip(
            ListFileEncodeRequest {
                content_kind: LIST_FILE_CONTENT_KIND_YMAP,
                content_schema_version: 3,
                entry_count: 5,
                additional_flags: 0,
                min_size_class: 4,
                header_metadata: metadata,
                body_stored: &body,
                body_uncompressed_len: 123,
                body_raw_hash: None,
                stable_file_id: None,
                import_settings_hash: None,
            },
            5,
        );
        assert_eq!(header.header_metadata_offset, 32);
        assert_eq!(header.header_metadata_len, metadata.len() as u64);
        assert_eq!(header.body_len, body.len() as u64);
        assert_eq!(header.body_uncompressed_len, 123);
        assert_eq!(header.entry_count, 5);
    }

    #[test]
    fn class_6_adds_full_body_hash() {
        let hash = [0xAB; 32];
        let header = round_trip(
            ListFileEncodeRequest {
                content_kind: LIST_FILE_CONTENT_KIND_YTD,
                content_schema_version: 1,
                entry_count: 118,
                additional_flags: 0,
                min_size_class: 4,
                header_metadata: &[],
                body_stored: &[1, 2],
                body_uncompressed_len: 8,
                body_raw_hash: Some(hash),
                stable_file_id: None,
                import_settings_hash: None,
            },
            6,
        );
        assert!(header.has_body_raw_hash());
        assert_eq!(header.body_raw_hash, hash);
    }

    #[test]
    fn class_7_adds_identity_fields() {
        let header = round_trip(
            ListFileEncodeRequest {
                content_kind: LIST_FILE_CONTENT_KIND_NEUI,
                content_schema_version: 9,
                entry_count: 2,
                additional_flags: 0,
                min_size_class: 4,
                header_metadata: &[],
                body_stored: &[9],
                body_uncompressed_len: 1,
                body_raw_hash: None,
                stable_file_id: Some(11),
                import_settings_hash: Some(12),
            },
            7,
        );
        assert_eq!(header.stable_file_id, 11);
        assert_eq!(header.import_settings_hash, 12);
    }

    #[test]
    fn v2_offsets_keep_type_id_and_flags_distinct() {
        let metadata = br#"{"schema":"metadata"}"#;
        let bytes = encode_list_file(ListFileEncodeRequest {
            content_kind: LIST_FILE_CONTENT_KIND_YFD,
            content_schema_version: 1,
            entry_count: 5,
            additional_flags: 0,
            min_size_class: 4,
            header_metadata: metadata,
            body_stored: &[1, 2, 3],
            body_uncompressed_len: 9,
            body_raw_hash: None,
            stable_file_id: None,
            import_settings_hash: None,
        })
        .unwrap();
        assert_eq!(
            u16::from_le_bytes([bytes[6], bytes[7]]) as u32,
            LIST_FILE_CONTENT_KIND_YFD
        );
        assert_eq!(
            u16::from_le_bytes([bytes[8], bytes[9]]),
            LIST_FILE_FLAG_BODY_DEFLATE | LIST_FILE_FLAG_HEADER_METADATA
        );
    }

    #[test]
    fn metadata_flag_requires_a_real_metadata_region() {
        let mut bytes = encode_list_file(ListFileEncodeRequest {
            content_kind: LIST_FILE_CONTENT_KIND_YSC,
            content_schema_version: 1,
            entry_count: 0,
            additional_flags: 0,
            min_size_class: 5,
            header_metadata: &[],
            body_stored: &[1, 2, 3],
            body_uncompressed_len: 9,
            body_raw_hash: None,
            stable_file_id: None,
            import_settings_hash: None,
        })
        .unwrap();
        let flags = LIST_FILE_FLAG_BODY_DEFLATE | LIST_FILE_FLAG_HEADER_METADATA;
        bytes[8..10].copy_from_slice(&flags.to_le_bytes());
        let error = parse_list_file_header(&bytes).unwrap_err();
        assert!(
            error.contains("metadata flag set"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn size_class_is_bounded() {
        let mut bytes = vec![0_u8; 16];
        bytes[0..4].copy_from_slice(&LIST_FILE_MAGIC_NEF8);
        bytes[4] = LIST_FILE_VERSION as u8;
        bytes[5] = 31;
        assert!(parse_list_file_header(&bytes).is_err());
    }
