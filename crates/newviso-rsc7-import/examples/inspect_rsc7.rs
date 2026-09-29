use newviso_resource_runtime::ResourceDecoder;
use newviso_semantic_assets::{SemanticCollisionDecoder, SemanticModelDecoder};

fn main() -> Result<(), String> {
    let model = SemanticModelDecoder;
    let collision = SemanticCollisionDecoder;

    if model.name().is_empty() || collision.name().is_empty() {
        return Err("semantic asset decoder registration name is empty".to_owned());
    }
    Ok(())
}
