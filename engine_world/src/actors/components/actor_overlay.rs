// Copyright Rob Gage 2026

use bevy_ecs::component::Component;

/// Shape of a cell-aligned area drawn over a scene around an actor's position
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum ActorOverlayShape {
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

/// A cell-aligned area drawn over the scene around the actor's position, tinting every cell whose
/// center lies inside it
#[derive(Component, Copy, Clone, Debug, PartialEq)]
pub struct ActorOverlayComponent {
    /// Shape of the area in tiles
    pub shape: ActorOverlayShape,
    /// Packed RGBA8 color, where alpha is the opacity
    pub color: u32,
}
