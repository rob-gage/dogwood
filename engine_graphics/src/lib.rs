// Copyright Rob Gage 2026

//! Rendering-facing material, camera, and scene resource types.

pub(crate) use dogwood_engine_compute as engine_compute;

mod camera;
mod color;
mod material_appearance;
mod material_graphics;
mod scene_graphics;

pub use camera::Camera;
pub use color::Color;
pub use material_appearance::{MaterialAppearance, MaterialOptics};
pub use material_graphics::{
    MaterialGraphics, MaterialGraphicsCellularDynamic, MaterialGraphicsCellularStatic,
    MaterialGraphicsFluid, MaterialGraphicsGas,
};
pub use scene_graphics::{
    SceneActorGraphics, SceneGraphics, SceneOverlay, SceneSpriteGraphics, SceneSpriteSheetGraphics,
};
