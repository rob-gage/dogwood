// Copyright Rob Gage 2026

mod template_game;
mod template_scene_generator;

pub use template_game::TemplateGame;

/// Source assets in development, indexed resources in distributed releases.
fn resource_bytes(id: &str) -> Vec<u8> {
    static RESOURCES: std::sync::OnceLock<engine::resources::Resources> = std::sync::OnceLock::new();
    RESOURCES.get_or_init(|| engine::resources::Resources::discover(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets")
    ).expect("resource catalogue")).load(id).unwrap_or_else(|e|panic!("resource {id}: {e}"))
}

macro_rules! asset {
    ($path:literal) => { {
        static BYTES: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
        BYTES.get_or_init(|| crate::resource_bytes($path)).as_slice()
    } };
}
pub(crate) use asset;
