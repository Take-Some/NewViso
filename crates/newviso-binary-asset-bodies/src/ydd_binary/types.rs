//! Public binary YDD document model and selection helpers.

/// Legacy static-mesh body schema. Kept readable for existing resident assets.
pub const YDD_BINARY_SCHEMA_VERSION_V2: u32 = 2;
/// Four-influence skin schema retained for resident assets.
pub const YDD_BINARY_SCHEMA_VERSION_V3: u32 = 3;
/// Current body schema. V4 extends the optional skin stream from four to eight
/// influences while preserving V2/V3 decoding.
pub const YDD_BINARY_SCHEMA_VERSION_V4: u32 = 4;
/// Optional per-mesh inline PBR material payload; V2-V4 remain readable.
pub const YDD_BINARY_SCHEMA_VERSION: u32 = 5;
pub const YDD_BINARY_ENCODING: &str = "newengine.ydd.binary_mesh.v5";
pub const YDD_BINARY_ENCODING_V4: &str = "newengine.ydd.binary_mesh.v4";
pub const YDD_BINARY_ENCODING_V3: &str = "newengine.ydd.binary_mesh.v3";
pub const YDD_BINARY_ENCODING_V2: &str = "newengine.ydd.binary_mesh.v2";
pub const YDD_BINARY_CONTRACT_SPEC: newviso_contract_api::ContractSpec =
    newviso_contract_api::ContractSpec::new(
        "asset.ydd.body",
        newviso_contract_api::ContractKind::Schema,
        newviso_contract_api::ContractVersion::major(YDD_BINARY_SCHEMA_VERSION as u16),
        newviso_contract_api::ContractCompatibility::Exact,
        "newengine-asset-format-nef8",
        Some(YDD_BINARY_ENCODING),
    );

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct YddBinaryVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv0: [f32; 2],
}

/// Eight-influence linear-blend-skinning source for one vertex.
///
/// The first quartet is wire-compatible with YDD V3. V4 adds `joints_extra` and
/// `weights_extra`; V3 decoding initializes that second quartet to zero.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct YddBinarySkinVertex {
    pub joints: [u16; 4],
    pub weights: [f32; 4],
    pub joints_extra: [u16; 4],
    pub weights_extra: [f32; 4],
}

impl YddBinarySkinVertex {
    #[inline]
    pub const fn four(joints: [u16; 4], weights: [f32; 4]) -> Self {
        Self {
            joints,
            weights,
            joints_extra: [0; 4],
            weights_extra: [0.0; 4],
        }
    }

    #[inline]
    pub fn total_weight(&self) -> f32 {
        self.weights
            .iter()
            .chain(self.weights_extra.iter())
            .copied()
            .sum()
    }

    #[inline]
    pub fn uses_extra_influences(&self) -> bool {
        self.weights_extra.iter().any(|weight| *weight > 0.0)
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct YddBinaryMesh {
    pub name: String,
    pub material_ref: Option<String>,
    /// Optional embedded PBR material; encoded explicitly in YDD V5.
    /// Earlier V2-V4 documents decode with None.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inline_material: Option<serde_json::Value>,
    pub bounds_min: [f32; 3],
    pub bounds_max: [f32; 3],
    pub vertices: Vec<YddBinaryVertex>,
    /// Optional V3/V4 skin stream. When present, it must contain exactly one record
    /// for every base vertex.
    pub skin: Option<Vec<YddBinarySkinVertex>>,
    pub indices: Vec<u32>,
}

impl YddBinaryMesh {
    #[inline]
    pub fn material_slot(&self) -> String {
        if let Some(reference) = self.material_ref.as_deref() {
            if let Some((_, selector)) = reference.rsplit_once('@') {
                let selector = selector.trim();
                if !selector.is_empty() {
                    return selector.to_owned();
                }
            }
        }
        let name = self.name.trim();
        if name.is_empty() {
            "material".to_owned()
        } else {
            name.to_owned()
        }
    }

    #[inline]
    pub fn is_skinned(&self) -> bool {
        self.skin.as_ref().is_some_and(|skin| !skin.is_empty())
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct YddBinaryEntry {
    pub name: String,
    pub source_path: String,
    pub properties_ref: Option<String>,
    pub bounds_min: [f32; 3],
    pub bounds_max: [f32; 3],
    /// Optional V3 affine transform from the authored skeleton/skin source space
    /// into the baked model vertex space. Column-major 4x4.
    pub skin_source_to_model: Option<[f32; 16]>,
    pub meshes: Vec<YddBinaryMesh>,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct YddBinaryDocument {
    pub entries: Vec<YddBinaryEntry>,
}

impl YddBinaryDocument {
    pub fn select_entry(
        &self,
        selector: Option<&str>,
        allow_single_entry_fallback: bool,
    ) -> Result<&YddBinaryEntry, String> {
        match selector.map(str::trim).filter(|value| !value.is_empty()) {
            Some(selector) => self
                .entries
                .iter()
                .find(|entry| entry.name.eq_ignore_ascii_case(selector))
                .or_else(|| {
                    (allow_single_entry_fallback && self.entries.len() == 1)
                        .then(|| &self.entries[0])
                })
                .ok_or_else(|| format!("binary YDD selector '{selector}' was not found")),
            None => self
                .entries
                .first()
                .ok_or_else(|| "binary YDD contains no entries".to_owned()),
        }
    }

    pub fn select_mesh(
        &self,
        selector: Option<&str>,
        allow_single_entry_fallback: bool,
    ) -> Result<(&YddBinaryEntry, &YddBinaryMesh), String> {
        if let Some(selector) = selector.map(str::trim).filter(|value| !value.is_empty()) {
            if let Some(entry) = self
                .entries
                .iter()
                .find(|entry| entry.name.eq_ignore_ascii_case(selector))
            {
                return entry
                    .meshes
                    .first()
                    .map(|mesh| (entry, mesh))
                    .ok_or_else(|| format!("binary YDD entry '{selector}' contains no meshes"));
            }
            for entry in &self.entries {
                if let Some(mesh) = entry
                    .meshes
                    .iter()
                    .find(|mesh| mesh.name.eq_ignore_ascii_case(selector))
                {
                    return Ok((entry, mesh));
                }
            }
        }
        let entry = self.select_entry(selector, allow_single_entry_fallback)?;
        entry
            .meshes
            .first()
            .map(|mesh| (entry, mesh))
            .ok_or_else(|| format!("binary YDD entry '{}' contains no meshes", entry.name))
    }
}
