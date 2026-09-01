// Copyright Rob Gage 2026

use crate::renders::{
    SceneRenderer,
    UserInterfaceRenderer
};
use super::Game;

/// Renders the current game frame, with the user interface over the scene
pub fn render_game<G: Game>(
    game: &mut G,
    command_encoder: &mut wgpu::CommandEncoder,
    target: &wgpu::TextureView,
) {
    SceneRenderer::new().render(game.scene(), command_encoder, target);
    UserInterfaceRenderer::new().render(
        game.user_interface_context(),
        command_encoder,
        target,
    );
}
