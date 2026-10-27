// Copyright Rob Gage 2026

mod scene_renderer;
mod scene_renderer_cellular_pass;
mod scene_renderer_fluid_pass;
mod scene_renderer_gas_pass;
mod scene_renderer_lighting_pass;
mod scene_renderer_overlay_pass;
mod scene_renderer_pressure_pass;
mod scene_renderer_temperature_pass;
mod scene_renderer_sprites_pass;

pub use scene_renderer::{
    SceneRenderer,
    SceneRendererViewMode,
    TEMPERATURE_VIEW_AMBIENT,
    TEMPERATURE_VIEW_MAXIMUM,
};
