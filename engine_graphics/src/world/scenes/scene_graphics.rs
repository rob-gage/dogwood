// Copyright Rob Gage 2026

use super::{
    SceneGraphicsCellular,
    SceneGraphicsFluid,
    SceneGraphicsGas,
    SceneGraphicsOverlay,
    SceneGraphicsPressure,
    SceneGraphicsSprites,
};

/// Borrowed graphical representation of one scene for one rendered frame
#[derive(Clone, Debug)]
pub struct SceneGraphics<'a> {
    /// Width of the area visible around the camera in tiles
    pub visible_width: f32,
    /// Height of the area visible around the camera in tiles
    pub visible_height: f32,
    /// Number of cells along each side of a tile
    pub cellular_tile_size: u32,
    /// Camera-relative origin of the cellular grid in tiles
    pub cell_grid_origin: [f32; 2],
    /// Resident cellular content, absent until a cellular region has streamed onto the GPU
    pub cellular: Option<SceneGraphicsCellular<'a>>,
    /// Resident fluid raster, absent until a cellular region has streamed onto the GPU
    pub fluid: Option<SceneGraphicsFluid<'a>>,
    /// Resident gas cells, absent until a cellular region has streamed onto the GPU
    pub gas: Option<SceneGraphicsGas<'a>>,
    /// Resident pressure, absent until a cellular region has streamed onto the GPU
    pub pressure: Option<SceneGraphicsPressure<'a>>,
    /// Background planes with physical depth, drawn before gameplay
    pub parallax: Vec<super::SceneGraphicsParallax<'a>>,
    /// Sprite content drawn around the cellular content by depth
    pub sprites: SceneGraphicsSprites<'a>,
    /// Cell-aligned areas drawn over the scene
    pub overlays: Vec<SceneGraphicsOverlay>,
}
