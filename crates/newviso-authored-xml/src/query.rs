use crate::XmlNode;

#[inline]
pub fn root_schema(root: XmlNode<'_, '_>) -> String {
    xml_attr_any(root, &["schema"]).unwrap_or_default()
}

#[inline]
pub fn root_has_any_name(root: XmlNode<'_, '_>, names: &[&str]) -> bool {
    names.iter().any(|name| root.has_tag_name(*name))
}

pub fn xml_attr_any(node: XmlNode<'_, '_>, names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| {
        node.attribute(*name)
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
    })
}

pub fn xml_attr_bool_any(node: XmlNode<'_, '_>, names: &[&str]) -> Option<bool> {
    xml_attr_any(node, names).map(|value| {
        matches!(
            value.to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

pub fn xml_attr_u32_any(node: XmlNode<'_, '_>, names: &[&str]) -> Option<u32> {
    xml_attr_any(node, names).and_then(|value| value.parse::<u32>().ok())
}

pub fn xml_attr_u64_any(node: XmlNode<'_, '_>, names: &[&str]) -> Option<u64> {
    xml_attr_any(node, names).and_then(|value| value.parse::<u64>().ok())
}

pub fn xml_attr_f32_any(node: XmlNode<'_, '_>, names: &[&str]) -> Option<f32> {
    xml_attr_any(node, names).and_then(|value| value.parse::<f32>().ok())
}

pub fn xml_child<'a, 'input>(node: XmlNode<'a, 'input>, name: &str) -> Option<XmlNode<'a, 'input>> {
    node.children()
        .find(|child| child.is_element() && child.tag_name().name().eq_ignore_ascii_case(name))
}

pub fn xml_children_named<'a, 'input>(
    node: XmlNode<'a, 'input>,
    name: &str,
) -> Vec<XmlNode<'a, 'input>> {
    node.children()
        .filter(|child| child.is_element() && child.tag_name().name().eq_ignore_ascii_case(name))
        .collect()
}

pub fn xml_tags(node: XmlNode<'_, '_>, container: &str) -> Vec<String> {
    xml_child(node, container)
        .map(|tags| {
            tags.children()
                .filter(|child| child.is_element())
                .filter_map(|child| {
                    xml_attr_any(child, &["value", "name"])
                        .or_else(|| child.text().map(str::trim).map(ToOwned::to_owned))
                })
                .filter(|value| !value.trim().is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use crate::parse_xml_document;

    use super::*;

    #[test]
    fn typed_attribute_helpers_share_alias_lookup() {
        let doc = parse_xml_document(
            r#"<Root enabled="yes" count="42" distance="2.5" />"#,
            "query_test",
        )
        .expect("valid XML");
        let root = doc.root_element();

        assert_eq!(xml_attr_bool_any(root, &["active", "enabled"]), Some(true));
        assert_eq!(xml_attr_u32_any(root, &["count"]), Some(42));
        assert_eq!(xml_attr_f32_any(root, &["distance"]), Some(2.5));
    }
}
