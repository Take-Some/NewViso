#![forbid(unsafe_op_in_unsafe_fn)]

/// Canonical VFS asset reference syntax: `<logical-path>[@entry]`.
///
/// The logical path is always VFS-facing and never a physical filesystem path.
/// The optional `entry` selector is shared by dictionary/container codecs:
/// `.ytd@texture_name`, `.ydd@drawable_name`, `.ytyp@archetype_name`,
/// and `.nemat@material_name`.
pub const ASSET_REF_ENTRY_SEPARATOR: char = '@';

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
#[serde(default)]
pub struct AssetReference {
    pub logical_path: String,
    pub entry: Option<String>,
    pub canonical: String,
}
impl AssetReference {
    pub fn parse(value: &str) -> Result<Self, String> {
        parse_asset_reference(value)
    }

    #[inline]
    pub fn extension(&self) -> Option<&str> {
        self.logical_path
            .rsplit_once('.')
            .map(|(_, ext)| ext)
            .filter(|ext| !ext.is_empty())
    }

    #[inline]
    pub fn has_extension(&self, extension: &str) -> bool {
        let expected = extension.trim().trim_start_matches('.');
        self.extension()
            .map(|ext| ext.eq_ignore_ascii_case(expected))
            .unwrap_or(false)
    }

    #[inline]
    pub fn require_entry(&self) -> Result<(), String> {
        if self
            .entry
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .is_some()
        {
            Ok(())
        } else {
            Err(format!(
                "asset reference '{}' requires @entry selector",
                self.canonical
            ))
        }
    }
}

pub fn parse_asset_reference(value: &str) -> Result<AssetReference, String> {
    let canonical = normalize_asset_reference_text(value)?;
    let (logical_path, entry) = match canonical.rsplit_once(ASSET_REF_ENTRY_SEPARATOR) {
        Some((path, entry)) => {
            let entry = entry.trim();
            if entry.is_empty() {
                return Err(format!(
                    "asset reference '{}' has empty @entry selector",
                    canonical
                ));
            }
            (path.trim().to_owned(), Some(entry.to_owned()))
        }
        None => (canonical.clone(), None),
    };
    if logical_path.is_empty() {
        return Err("asset reference logical path is empty".to_owned());
    }
    validate_vfs_logical_path(&logical_path)?;
    let canonical = match entry.as_deref() {
        Some(entry) => format!("{logical_path}@{entry}"),
        None => logical_path.clone(),
    };
    Ok(AssetReference {
        logical_path,
        entry,
        canonical,
    })
}

pub fn normalize_asset_reference_text(value: &str) -> Result<String, String> {
    let mut s = value.trim().replace('\\', "/");
    while let Some(rest) = s.strip_prefix("./") {
        s = rest.to_owned();
    }
    s = s.trim_start_matches('/').to_owned();
    while s.contains("//") {
        s = s.replace("//", "/");
    }
    if s.is_empty() {
        return Err("asset reference is empty".to_owned());
    }
    Ok(s)
}

pub fn validate_vfs_logical_path(path: &str) -> Result<(), String> {
    let s = path.trim();
    if s.is_empty() {
        return Err("VFS logical path is empty".to_owned());
    }
    if s.contains(':') || s.starts_with('/') || s.starts_with('\\') || s.starts_with("//") {
        return Err(format!(
            "asset reference must be VFS logical path, not physical/absolute path: '{path}'"
        ));
    }
    if s.split('/').any(|part| part == "..") {
        return Err(format!(
            "asset reference must not contain parent traversal: '{path}'"
        ));
    }
    Ok(())
}

pub fn is_raw_source_image_extension(extension: &str) -> bool {
    matches!(
        extension
            .trim()
            .trim_start_matches('.')
            .to_ascii_lowercase()
            .as_str(),
        "png" | "jpg" | "jpeg" | "tga" | "bmp" | "dds" | "webp" | "tif" | "tiff"
    )
}

const RETIRED_TEXTURE_DICTIONARY_EXTENSION: &str = concat!("ne", "ytd");

pub fn is_raw_source_image_reference(value: &str) -> bool {
    parse_asset_reference(value)
        .ok()
        .and_then(|reference| reference.extension().map(|ext| ext.to_owned()))
        .map(|ext| is_raw_source_image_extension(&ext))
        .unwrap_or(false)
}

pub fn is_retired_texture_dictionary_reference(value: &str) -> bool {
    parse_asset_reference(value)
        .map(|reference| reference.has_extension(RETIRED_TEXTURE_DICTIONARY_EXTENSION))
        .unwrap_or_else(|_| {
            value
                .trim()
                .to_ascii_lowercase()
                .contains(&format!(".{RETIRED_TEXTURE_DICTIONARY_EXTENSION}"))
        })
}

pub fn require_asset_reference_extension(
    value: &str,
    extensions: &[&str],
    require_entry: bool,
) -> Result<AssetReference, String> {
    let reference = parse_asset_reference(value)?;
    if require_entry {
        reference.require_entry()?;
    }
    if extensions.iter().any(|ext| reference.has_extension(ext)) {
        return Ok(reference);
    }
    let expected = extensions
        .iter()
        .map(|ext| format!(".{}", ext.trim().trim_start_matches('.')))
        .collect::<Vec<_>>()
        .join("|");
    Err(format!(
        "asset reference '{}' must use extension {expected}",
        reference.canonical
    ))
}
