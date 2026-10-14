// Copyright Rob Gage 2026

use crate::sprites::SpriteSheet;
use std::sync::Arc;

/// Graphics properties for a cellular static material
pub struct MaterialGraphicsCellularStatic {
    /// Sprite sheet holding the albedo frame
    pub sprite_sheet: Arc<SpriteSheet>,
    /// Albedo frame repeated across the material
    pub frame: u32,
    /// Horizontal texture scale in cells per pixel; `1.0` maps each pixel to one cell
    pub width: f32,
    /// Vertical texture scale in cells per pixel; `1.0` maps each pixel to one cell
    pub height: f32,
}
