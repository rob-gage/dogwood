// Copyright Rob Gage 2026

/// Render view configuration measured in tiles
pub struct Camera {
    /// The width of the camera view in floating-point units
    pub width: f32,
    /// The height of the camera view in floating-point units
    pub height: f32,
    /// The number of tiles covered by each floating-point unit of camera width and height; a
    /// `16.0` by `9.0` camera at a zoom of `2.0` covers an area of `32.0` by `18.0` tiles
    pub zoom: f32,
    /// The acceleration at which the camera follows its target in tiles per second squared
    pub follow_acceleration: f32,
    /// The maximum speed at which the camera follows its target in tiles per second
    pub follow_speed: f32,
    /// The maximum distance the camera may follow behind its target in tiles
    pub follow_distance_maximum: f32,
}
