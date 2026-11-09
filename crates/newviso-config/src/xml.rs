use serde_json::Value;
use std::{collections::BTreeMap, path::Path};

#[derive(Debug)]
struct EngineConfigXmlNode {
    name: String,
    attrs: BTreeMap<String, String>,
    children: Vec<EngineConfigXmlNode>,
}

pub fn parse_engine_config_xml(bytes: &[u8], path: &Path) -> Result<Value, String> {
    use quick_xml::{events::Event, Reader, XmlVersion};

    fn attrs(
        event: &quick_xml::events::BytesStart<'_>,
    ) -> Result<BTreeMap<String, String>, String> {
        let mut out = BTreeMap::new();
        for attribute in event.attributes().with_checks(false) {
            let attribute = attribute.map_err(|error| format!("XML attribute error: {error}"))?;
            let key = attribute.key.into_inner().to_owned();
            let value = attribute
                .normalized_value(XmlVersion::Implicit1_0)
                .map_err(|error| format!("XML attribute decode failed: {error}"))?
                .into_owned();
            out.insert(key, value);
        }
        Ok(out)
    }

    fn scalar(raw: &str) -> Value {
        let raw = raw.trim();
        if raw.eq_ignore_ascii_case("true") {
            return Value::Bool(true);
        }
        if raw.eq_ignore_ascii_case("false") {
            return Value::Bool(false);
        }
        if let Ok(value) = raw.parse::<i64>() {
            return Value::Number(value.into());
        }
        if let Ok(value) = raw.parse::<u64>() {
            return Value::Number(value.into());
        }
        if let Ok(value) = raw.parse::<f64>() {
            if let Some(value) = serde_json::Number::from_f64(value) {
                return Value::Number(value);
            }
        }
        Value::String(raw.to_owned())
    }

    fn node_value(node: EngineConfigXmlNode) -> Result<Value, String> {
        let kind = node.attrs.get("type").map(String::as_str);
        if kind == Some("array") {
            return node
                .children
                .into_iter()
                .map(node_value)
                .collect::<Result<Vec<_>, _>>()
                .map(Value::Array);
        }
        if kind == Some("object") && node.children.is_empty() {
            return Ok(Value::Object(serde_json::Map::new()));
        }
        if node
            .attrs
            .get("null")
            .is_some_and(|value| value.eq_ignore_ascii_case("true"))
        {
            return Ok(Value::Null);
        }

        let axes = ["x", "y", "z", "w"]
            .into_iter()
            .filter_map(|axis| node.attrs.get(axis).map(|value| scalar(value)))
            .collect::<Vec<_>>();
        if axes.len() >= 2 {
            return Ok(Value::Array(axes));
        }

        if node.children.is_empty() {
            if let Some(value) = node.attrs.get("value") {
                return Ok(scalar(value));
            }
            if node.attrs.is_empty() {
                return Ok(Value::Object(serde_json::Map::new()));
            }
        }

        let mut object = serde_json::Map::new();
        for (key, value) in node.attrs {
            if key != "type" && key != "null" && key != "value" {
                object.insert(key, scalar(&value));
            }
        }
        for child in node.children {
            let key = child.name.clone();
            let value = node_value(child)?;
            if let Some(previous) = object.remove(&key) {
                let mut values = match previous {
                    Value::Array(values) => values,
                    other => vec![other],
                };
                values.push(value);
                object.insert(key, Value::Array(values));
            } else {
                object.insert(key, value);
            }
        }
        Ok(Value::Object(object))
    }

    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(true);
    let mut buffer = Vec::new();
    let mut stack = Vec::<EngineConfigXmlNode>::new();
    let mut root = None::<EngineConfigXmlNode>;

    loop {
        match reader
            .read_event_into(&mut buffer)
            .map_err(|error| format!("XML read failed path='{}': {error}", path.display()))?
        {
            Event::Start(event) => {
                let name = event.name().into_inner().to_owned();
                stack.push(EngineConfigXmlNode {
                    name,
                    attrs: attrs(&event)?,
                    children: Vec::new(),
                });
            }
            Event::Empty(event) => {
                let name = event.name().into_inner().to_owned();
                let node = EngineConfigXmlNode {
                    name,
                    attrs: attrs(&event)?,
                    children: Vec::new(),
                };
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(node);
                } else if root.replace(node).is_some() {
                    return Err("XML contains more than one root element".to_owned());
                }
            }
            Event::End(_) => {
                let node = stack
                    .pop()
                    .ok_or_else(|| "XML closing element has no matching start".to_owned())?;
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(node);
                } else if root.replace(node).is_some() {
                    return Err("XML contains more than one root element".to_owned());
                }
            }
            Event::Text(text) if !text.as_ref().chars().all(char::is_whitespace) => {
                return Err(
                    "engine defaults XML uses attribute values; non-whitespace text is not allowed"
                        .to_owned(),
                );
            }
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }

    if !stack.is_empty() {
        return Err("XML ended with unclosed elements".to_owned());
    }
    let root = root.ok_or_else(|| "XML contains no root element".to_owned())?;
    node_value(root)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn existing_xml_convention_preserves_arrays_null_and_utf8() {
        let value=parse_engine_config_xml(br#"<Settings schema="v1"><window title="test"/><unset null="true"/><items type="array"><item value="1"/><item value="true"/></items></Settings>"#,Path::new("test.xml")).unwrap();
        assert_eq!(value["items"], serde_json::json!([1, true]));
        assert!(value["unset"].is_null());
        for bytes in [
            b"<a>unexpected text</a>".as_slice(),
            b"<a><b></a>",
            b"<a/><b/>",
        ] {
            assert!(parse_engine_config_xml(bytes, Path::new("broken.xml")).is_err());
        }
    }
}
