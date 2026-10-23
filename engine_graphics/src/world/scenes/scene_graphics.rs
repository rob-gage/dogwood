// Copyright Rob Gage 2026

use super::{
    SceneGraphicsCellular,
    SceneGraphicsFluid,
    SceneGraphicsGas,
    SceneGraphicsSprites,
};

/// Borrowed graphical representation of one scene for one rendered frame
#[derive(Clone, Debug)]
pub struct SceneGraphics<'a> {
    /// Width of the area visible around the camera in tiles
    pub visible_width: f32,
    /// Height of the area visible around the camera in tiles
    pub visible_height: f32,
    /// Resident cellular content, absent until a cellular region has streamed onto the GPU
    pub cellular: Option<SceneGraphicsCellular<'a>>,
    /// Resident fluid raster, absent until a cellular region has streamed onto the GPU
    pub fluid: Option<SceneGraphicsFluid<'a>>,
    /// Resident gas cells, absent until a cellular region has streamed onto the GPU
    pub gas: Option<SceneGraphicsGas<'a>>,
    /// Sprite content drawn around the cellular content by depth
    pub sprites: SceneGraphicsSprites<'a>,
}
