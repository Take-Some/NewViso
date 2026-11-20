#![forbid(unsafe_op_in_unsafe_fn)]
pub const DRAWABLE_DICTIONARY_ASSET_KIND: &str = "drawable_dictionary";
pub const DRAWABLE_DICTIONARY_CONTAINER: &str = "newengine.listfile.nef8.ydd";
pub const DRAWABLE_DICTIONARY_MANIFEST_SCHEMA: &str = "newengine.drawable_dictionary.manifest.v1";
pub const TEXTURE_DICTIONARY_ASSET_KIND: &str = "texture_dictionary";
pub const TEXTURE_DICTIONARY_CONTAINER: &str = "newengine.listfile.nef8.ytd";
pub const TEXTURE_DICTIONARY_MANIFEST_SCHEMA: &str = "newengine.texture_dictionary.manifest.v2";

mod drawable;
mod texture_dictionary;
pub use drawable::*;
pub use texture_dictionary::*;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn manifest_schemas_preserve_prior_wire_ids() {
        assert_eq!(DrawableDictionaryManifest::default().schema,DRAWABLE_DICTIONARY_MANIFEST_SCHEMA);
        assert_eq!(TextureDictionaryManifest::default().schema,TEXTURE_DICTIONARY_MANIFEST_SCHEMA);
    }
}
