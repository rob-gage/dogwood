// Copyright Rob Gage 2026

use crate::sprites::SpriteSheet;
use std::sync::Arc;

/// Graphics properties for a cellular static material
pub struct MaterialGraphicsCellularStatic {
    /// Sprite sheet holding the albedo frame
    pub sprite_sheet: Arc<SpriteSheet>,
    /// Albedo frame repeated across the material
    pub frame: u32,
    /// Width in tiles covered by one repetition of the frame
    pub width: f32,
    /// Height in tiles covered by one repetition of the frame
    pub height: f32,
}
