// Copyright Rob Gage 2026

use super::MaterialGraphics;

/// Scene graphics information that is passed to the renderer
pub struct SceneGraphics<'a> {
    /// The graphics properties for every material in the scene
    pub material_graphics: &'a MaterialGraphics,
}
