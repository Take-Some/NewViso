use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, sync::OnceLock};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PropertyKind {
    Number,
    Integer,
    String,
    Boolean,
    Vec3,
    Color4,
    Asset,
    Enum,
    Object,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PropertyDescriptor {
    pub path: String,
    pub label: String,
    pub kind: PropertyKind,
    #[serde(default)]
    pub min: Option<f64>,
    #[serde(default)]
    pub max: Option<f64>,
    #[serde(default)]
    pub choices: Vec<String>,
}

impl PropertyDescriptor {
    pub fn validate(&self, value: &Value) -> Result<(), String> {
        let number = |v: &Value| {
            v.as_f64().is_some_and(|n| {
                n.is_finite()
                    && (n as f32).is_finite()
                    && self.min.is_none_or(|min| n >= min)
                    && self.max.is_none_or(|max| n <= max)
            })
        };
        let valid = match self.kind {
            PropertyKind::Object => value.is_object(),
            PropertyKind::Number => number(value),
            PropertyKind::Integer => value.as_u64().is_some() && number(value),
            PropertyKind::String => value.is_string(),
            PropertyKind::Boolean => value.is_boolean(),
            PropertyKind::Vec3 | PropertyKind::Color4 => value.as_array().is_some_and(|a| {
                a.len()
                    == (if matches!(self.kind, PropertyKind::Vec3) {
                        3
                    } else {
                        4
                    })
                    && a.iter().all(number)
            }),
            PropertyKind::Asset => value.as_str().is_some_and(|s| {
                !s.trim().is_empty()
                    && !s.contains('\\')
                    && !s.starts_with('/')
                    && !s.contains(':')
                    && !s.split('/').any(|p| p == "..")
            }),
            PropertyKind::Enum => value
                .as_str()
                .is_some_and(|s| self.choices.iter().any(|c| c == s)),
        };
        if valid {
            Ok(())
        } else {
            Err(format!(
                "{}: invalid {:?} value {}",
                self.path, self.kind, value
            ))
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentDescriptor {
    pub id: String,
    pub label: String,
    pub defaults: Value,
    pub properties: Vec<PropertyDescriptor>,
    #[serde(default)]
    pub entity_kinds: Vec<String>,
    #[serde(default)]
    pub mesh_primitives: Vec<String>,
}

impl ComponentDescriptor {
    pub fn applies_to(&self, entity: &Value) -> bool {
        (self.entity_kinds.is_empty()
            || entity["kind"]
                .as_str()
                .is_some_and(|kind| self.entity_kinds.iter().any(|k| k == kind)))
            && (self.mesh_primitives.is_empty()
                || entity
                    .pointer("/mesh/primitive")
                    .and_then(Value::as_str)
                    .is_some_and(|primitive| self.mesh_primitives.iter().any(|p| p == primitive)))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntityTypeDescriptor {
    pub id: String,
    pub label: String,
    pub template: Value,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SceneTypeRegistry {
    pub components: BTreeMap<String, ComponentDescriptor>,
    pub types: BTreeMap<String, EntityTypeDescriptor>,
}

impl SceneTypeRegistry {
    pub fn builtin() -> &'static Self {
        static REGISTRY: OnceLock<SceneTypeRegistry> = OnceLock::new();
        REGISTRY.get_or_init(|| {
            let registry: Self = serde_json::from_str(include_str!("assets/scene-types.json"))
                .expect("built-in scene type registry");
            registry.validate().expect("consistent built-in registry");
            registry
        })
    }
    /// Registration is transactional; extensions cannot replace an existing contract.
    pub fn register(&mut self, extension: Self) -> Result<(), String> {
        extension.validate()?;
        if extension
            .components
            .keys()
            .any(|id| self.components.contains_key(id))
            || extension.types.keys().any(|id| self.types.contains_key(id))
        {
            return Err("type/component registration collision".into());
        }
        self.components.extend(extension.components);
        self.types.extend(extension.types);
        Ok(())
    }
    pub fn property(&self, path: &str) -> Option<&PropertyDescriptor> {
        self.components
            .values()
            .flat_map(|c| &c.properties)
            .find(|p| p.path == path)
    }
    pub fn validate(&self) -> Result<(), String> {
        let mut paths = std::collections::BTreeSet::new();
        for (id, component) in &self.components {
            if id != &component.id
                || id.is_empty()
                || id.contains('/')
                || !component.defaults.is_object()
            {
                return Err(format!("invalid component descriptor '{id}'"));
            }
            for p in &component.properties {
                if !p.path.starts_with(&format!("/{id}/"))
                    || !paths.insert(&p.path)
                    || p.min.zip(p.max).is_some_and(|(a, b)| a > b)
                    || matches!(p.kind, PropertyKind::Enum) && p.choices.is_empty()
                {
                    return Err(format!("invalid property descriptor '{}'", p.path));
                }
                if let Some(v) = component
                    .defaults
                    .pointer(p.path.strip_prefix(&format!("/{id}")).unwrap())
                {
                    p.validate(v)?;
                }
            }
        }
        for (id, ty) in &self.types {
            if id != &ty.id
                || id.is_empty()
                || !ty.template.is_object()
                || !ty.template["kind"].is_string()
            {
                return Err(format!("invalid entity type '{id}'"));
            }
        }
        Ok(())
    }
}
