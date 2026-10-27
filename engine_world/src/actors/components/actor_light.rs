// Copyright Rob Gage 2026

use bevy_ecs::component::Component;

/// A light without a sprite or material, read only from actors with a position component; lights
/// with nonfinite or negative values are skipped
#[derive(Component, Copy, Clone, Debug, PartialEq)]
pub struct ActorLightComponent {
    /// Linear RGB color of the emitted light
    pub color: [f32; 3],
    /// Emission multiplier; the light emits `color * intensity` per cell of its path
    pub intensity: f32,
    /// Radius of the emitting disc in tiles
    pub radius: f32,
}

impl Default for ActorLightComponent {
    fn default() -> Self {
        Self {
            color: [1.0, 1.0, 1.0],
            intensity: 1.0,
            radius: 0.5,
        }
    }
}
