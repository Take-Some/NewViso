use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaterialGraphNodeKind {
    Constant,
    TextureSample,
    Parameter,
    Multiply,
    Add,
    NormalMap,
    Fresnel,
    OutputSurface,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MaterialGraphPortRef {
    pub node: String,
    pub port: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MaterialGraphEdgeDto {
    pub from: MaterialGraphPortRef,
    pub to: MaterialGraphPortRef,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MaterialGraphNodeDto {
    pub id: String,
    pub kind: MaterialGraphNodeKind,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MaterialShaderGraphDto {
    pub schema: String,
    pub graph_id: String,
    #[serde(default)]
    pub source_asset: String,
    #[serde(default)]
    pub nodes: Vec<MaterialGraphNodeDto>,
    #[serde(default)]
    pub edges: Vec<MaterialGraphEdgeDto>,
    #[serde(default)]
    pub required_textures: Vec<String>,
    #[serde(default)]
    pub required_variants: Vec<String>,
}

impl MaterialShaderGraphDto {
    #[inline]
    pub fn empty(graph_id: impl Into<String>) -> Self {
        Self {
            schema: "newengine.render.material_shader_graph.v1".to_owned(),
            graph_id: graph_id.into(),
            source_asset: String::new(),
            nodes: Vec::new(),
            edges: Vec::new(),
            required_textures: Vec::new(),
            required_variants: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MaterialGraphValidationReport {
    pub graph_id: String,
    #[serde(default)]
    pub valid: bool,
    #[serde(default)]
    pub errors: Vec<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
}
