// Copyright Rob Gage 2026

use engine_physics::scenes::Scene;

/// Renders a `Scene`.
pub struct SceneRenderer;

impl SceneRenderer {

    /// Creates a `SceneRenderer`.
    pub const fn new() -> Self { Self }

    /// Clears the target with the scene's temporary background color
    pub fn render(
        &self,
        _scene: &Scene,
        command_encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
    ) {
        let _render_pass = command_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Scene renderer"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.02, g: 0.15, b: 0.06, a: 1.0, }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
    }

}
