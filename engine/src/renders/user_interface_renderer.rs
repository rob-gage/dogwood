// Copyright Rob Gage 2026

use engine_user_interface::UserInterfaceContext;

/// Renders the user interface on top of a scene.
pub struct UserInterfaceRenderer;

impl UserInterfaceRenderer {

    /// Creates a `UserInterfaceRenderer`.
    pub const fn new() -> Self { Self }

    /// Renders the user interface on top of a scene.
    pub fn render(
        &self,
        _user_interface_context: &UserInterfaceContext,
        _command_encoder: &mut wgpu::CommandEncoder,
        _target: &wgpu::TextureView,
    ) {
    }

}
