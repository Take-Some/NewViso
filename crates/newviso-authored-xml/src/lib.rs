#![forbid(unsafe_op_in_unsafe_fn)]

//! Strict authored XML parser shared by NewViso resource codecs.
//!
//! This is a current NewViso contract, independent of the retired NewEngine
//! SDK. The returned document borrows from the source buffer: no surprise
//! copies, substitution of fallback documents, or entity expansion.

pub fn parse_xml_body<'a>(body: &'a [u8], label: &str) -> Result<roxmltree::Document<'a>, String> {
    let utf8 = std::str::from_utf8(body)
        .map_err(|error| format!("{label}: invalid UTF-8 XML source: {error}"))?;
    roxmltree::Document::parse_with_options(
        utf8,
        roxmltree::ParsingOptions {
            allow_dtd: false,
            ..roxmltree::ParsingOptions::default()
        },
    )
    .map_err(|error| format!("{label}: invalid XML: {error}"))
}

pub fn root_has_any_name(node: roxmltree::Node<'_, '_>, accepted: &[&str]) -> bool {
    node.is_element() && accepted.iter().any(|name| node.tag_name().name() == *name)
}

pub type XmlNode<'a, 'input> = roxmltree::Node<'a, 'input>;
pub type XmlDocument<'input> = roxmltree::Document<'input>;

pub fn parse_xml_document<'a>(body: &'a str, label: &str) -> Result<XmlDocument<'a>, String> {
    parse_xml_body(body.as_bytes(), label)
}
pub fn body_is_xml(body: &[u8]) -> bool {
    std::str::from_utf8(body)
        .ok()
        .is_some_and(|text| text.trim_start_matches('\u{feff}').trim_start().starts_with('<'))
}
pub fn text_is_xml(text: &str) -> bool {
    text.trim_start_matches('\u{feff}').trim_start().starts_with('<')
}

mod query;
pub use query::{
    root_schema, xml_attr_any, xml_attr_bool_any, xml_attr_f32_any,
    xml_attr_u32_any, xml_attr_u64_any, xml_child, xml_children_named, xml_tags,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_material_and_ui_roots_without_rewriting_them() {
        let doc = parse_xml_body(
            br#"<NematMaterialLibrary><Materials><Material name="wall"/></Materials></NematMaterialLibrary>"#,
            "NEMAT",
        ).unwrap();
        let root = doc.root_element();
        assert!(root_has_any_name(
            root,
            &["NematMaterialLibrary", "MaterialLibrary"]
        ));
        assert_eq!(
            root.first_element_child().unwrap().tag_name().name(),
            "Materials"
        );

        let ui = parse_xml_body(b"<UiDocument><Entries/></UiDocument>", "NEUI").unwrap();
        assert_eq!(ui.root_element().tag_name().name(), "UiDocument");
    }

    #[test]
    fn rejects_invalid_xml_and_utf8_without_fallback() {
        assert!(parse_xml_body(&[0xff], "NEMAT")
            .unwrap_err()
            .contains("UTF-8"));
        assert!(parse_xml_body(b"<NematMaterialLibrary>", "NEMAT").is_err());
    }

    #[test]
    fn rejects_dtd_entity_expansion() {
        let malicious = b"<!DOCTYPE root [<!ENTITY x 'abc'>]><root>&x;</root>";
        assert!(parse_xml_body(malicious, "source").is_err());
    }

    #[test]
    fn root_match_is_exact_not_substring() {
        let doc = parse_xml_body(b"<NematMaterialLibraryBackup/>", "NEMAT").unwrap();
        assert!(!root_has_any_name(
            doc.root_element(),
            &["NematMaterialLibrary"]
        ));
    }
}
