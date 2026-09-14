// Copyright Rob Gage 2026

use engine_compute::Accelerator;
use engine_user_interface::UserInterfaceContext;

/// Renders the user interface on top of a scene.
pub struct UserInterfaceRenderer {
    renderer: Option<egui_wgpu::Renderer>,
}

impl UserInterfaceRenderer {

    /// Creates a `UserInterfaceRenderer`.
    pub const fn new() -> Self { Self { renderer: None } }

    /// Renders the user interface on top of a scene.
    pub fn render(
        &mut self,
        user_interface_context: &UserInterfaceContext,
        accelerator: &Accelerator,
        format: wgpu::TextureFormat,
        size: [u32; 2],
        command_encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
    ) {
        let mut output: egui::FullOutput = user_interface_context.take_output();
        if output.shapes.is_empty() && output.textures_delta.set.is_empty() { return; }
        let renderer: &mut egui_wgpu::Renderer =
            self.renderer.get_or_insert_with(|| egui_wgpu::Renderer::new(
                accelerator.wgpu_device(),
                format,
                egui_wgpu::RendererOptions::default(),
            ));
        let paint_jobs: Vec<egui::ClippedPrimitive> = user_interface_context.egui_context()
            .tessellate(output.shapes, output.pixels_per_point);
        let screen_descriptor: egui_wgpu::ScreenDescriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: size,
            pixels_per_point: output.pixels_per_point,
        };
        for (texture_id, image_deltas) in output.textures_delta.set.clone() {
            for image_delta in image_deltas {
                renderer.update_texture(
                    accelerator.wgpu_device(),
                    accelerator.wgpu_queue(),
                    texture_id,
                    &image_delta,
                );
            }
        }
        renderer.update_buffers(
            accelerator.wgpu_device(),
            accelerator.wgpu_queue(),
            command_encoder,
            &paint_jobs,
            &screen_descriptor,
        );
        let render_pass: wgpu::RenderPass =
            command_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("User Interface Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: accelerator.render_pass_timestamp_writes(
                    "User Interface Render Pass",
                ),
                occlusion_query_set: None,
                multiview_mask: None,
            });
        let mut render_pass: wgpu::RenderPass = render_pass.forget_lifetime();
        renderer.render(&mut render_pass, &paint_jobs, &screen_descriptor);
        for texture_id in output.textures_delta.free.clone() {
            renderer.free_texture(&texture_id);
        }
        output.textures_delta.clear();
    }

}
