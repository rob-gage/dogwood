// Copyright Rob Gage 2026

use bevy_ecs::component::Component;

/// Orthographic view centered on the actor, showing `width × height` tiles at zoom `1.0`
#[derive(Component, Copy, Clone, Debug, PartialEq)]
pub struct ActorCameraComponent {
    /// Visible width in cellular tiles at zoom `1.0`
    pub width: u32,
    /// Visible height in cellular tiles at zoom `1.0`
    pub height: u32,
    /// Multiplier of the visible area along each axis
    pub zoom: f32,
}

impl ActorCameraComponent {

    /// Returns the visible width in cellular tiles
    pub fn visible_width(&self) -> f32 { self.width as f32 * self.zoom }

    /// Returns the visible height in cellular tiles
    pub fn visible_height(&self) -> f32 { self.height as f32 * self.zoom }

}
