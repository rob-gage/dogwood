// Copyright Rob Gage 2026

/// Shape of a cell-aligned area tinted over a scene
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum SceneGraphicsOverlayShape {
    /// Circle whose radius is measured in tiles
    Circle {
        /// Radius of the circle in tiles
        radius: f32,
    },
    /// Centered rectangle whose dimensions are measured in tiles
    Rectangle {
        /// Width of the rectangle in tiles
        width: f32,
        /// Height of the rectangle in tiles
        height: f32,
    },
}

/// Cell-aligned area tinted over a scene, centered relative to its camera
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct SceneGraphicsOverlay {
    /// Horizontal center relative to the camera in tiles
    pub center_x: f32,
    /// Vertical center relative to the camera in tiles
    pub center_y: f32,
    /// Shape of the area in tiles
    pub shape: SceneGraphicsOverlayShape,
    /// Packed RGBA8 color, where alpha is the opacity
    pub color: u32,
}
