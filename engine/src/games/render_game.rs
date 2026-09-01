// Copyright Rob Gage 2026

use crate::renders::{
    SceneRenderer,
    UserInterfaceRenderer
};
use engine_compute::Accelerator;
use super::Game;

/// Renders the current game frame, with the user interface over the scene
pub fn render_game<G: Game>(
    game: &mut G,
    user_interface_renderer: &mut UserInterfaceRenderer,
    accelerator: &Accelerator,
    format: wgpu::TextureFormat,
    size: [u32; 2],
    command_encoder: &mut wgpu::CommandEncoder,
    target: &wgpu::TextureView,
) {
    SceneRenderer::new().render(game.scene(), command_encoder, target);
    user_interface_renderer.render(
        game.user_interface_context(),
        accelerator,
        format,
        size,
        command_encoder,
        target,
    );
}
