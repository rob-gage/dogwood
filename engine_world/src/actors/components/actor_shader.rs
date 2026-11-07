// Copyright Rob Gage 2026

use bevy_ecs::component::Component;

/// Draws an actor's soft body over the cells with a shader the game supplies, which keeps what the
/// actor looks like out of the engine. `fragment` is WGSL defining
/// `fn shade(input: ShadeInput) -> ShadeOutput`, where `ShadeInput` holds `position`, the point
/// being shaded relative to the camera in tiles; `center`, the middle of the body's outline;
/// `distance`, how far inside the outline the point lies in tiles; `depth`, the distance of the
/// outline's deepest point; `outward`, the direction toward the nearest part of the outline; and
/// `parameters`, this component's parameters. `ShadeOutput` holds a linear `albedo`, lit by the
/// scene, and an `emission` that is not
#[derive(Component, Clone, Debug, PartialEq)]
pub struct ActorShaderComponent {
    /// WGSL source of its `shade` function
    pub fragment: &'static str,
    /// Values for the shader, in meanings of the game's choosing
    pub parameters: [[f32; 4]; 4],
    /// Whether it is shaded and outlined in whole world cells, for art made of cells
    pub is_cell_snapped: bool,
}
