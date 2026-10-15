// Copyright Rob Gage 2026

//! Rendering of egui output into an existing wgpu target

/// egui-wgpu overlay renderer owned by the application render loop
///
/// The renderer uses egui-wgpu's default options, which means a single-sample
/// target and no depth or stencil attachment. The target view's format must
/// match the format supplied to [`Self::new`]. Prefer
/// `Rgba8Unorm` or `Bgra8Unorm` gamma-space targets for egui.
/// egui-wgpu 0.36.2 has no target-format mutation API, so a format change
/// requires creating a new [`UserInterfaceRenderer`] and a new egui context
/// together. Reusing the old context would not replay already-consumed texture
/// deltas into the new renderer.
///
/// [`Self::render`] consumes its [`egui::FullOutput`]. The caller must handle
/// platform output before calling it. Texture updates are applied before
/// tessellation and painting; the overlay pass uses [`wgpu::LoadOp::Load`],
/// preserves the target contents, and follows the caller's earlier passes.
/// Callback command buffers are returned in egui-wgpu callback order and are
/// never submitted here. Submit them before the command buffer finished from
/// the encoder passed to [`Self::render`]. Callback paint operations run in
/// shape order inside the overlay pass. Texture frees happen after that pass,
/// including when there are no shapes or the requested target size is zero.
pub struct UserInterfaceRenderer {
    renderer: egui_wgpu::Renderer,
}

impl UserInterfaceRenderer {

    /// Creates a single-sample renderer for the supplied color target format
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let renderer_options: egui_wgpu::RendererOptions =
            egui_wgpu::RendererOptions::default();
        let renderer: egui_wgpu::Renderer =
            egui_wgpu::Renderer::new(device, format, renderer_options);
        Self { renderer }
    }

    /// Borrows the inner renderer for texture and callback-resource access
    pub fn renderer(&self) -> &egui_wgpu::Renderer {
        &self.renderer
    }

    /// Mutably borrows the inner renderer for native textures and callback resources
    pub fn renderer_mut(&mut self) -> &mut egui_wgpu::Renderer {
        &mut self.renderer
    }

    /// Paints consumed egui output over an existing single-sample target
    pub fn render(
        &mut self,
        context: &egui::Context,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        output: egui::FullOutput,
        size_in_pixels: [u32; 2],
    ) -> Vec<wgpu::CommandBuffer> {
        let full_output: egui::FullOutput = output;
        let shapes: Vec<egui::epaint::ClippedShape> = full_output.shapes;
        let pixels_per_point: f32 = full_output.pixels_per_point;
        let mut textures_delta: egui::TexturesDelta = full_output.textures_delta;
        for (texture_id, image_deltas) in textures_delta.set.drain() {
            for image_delta in image_deltas {
                self.renderer
                    .update_texture(device, queue, texture_id, &image_delta);
            }
        }
        if size_in_pixels[0] == 0 || size_in_pixels[1] == 0 {
            for texture_id in textures_delta.free.drain() {
                self.renderer.free_texture(&texture_id);
            }
            return Vec::new();
        }
        let paint_jobs: Vec<egui::ClippedPrimitive> =
            context.tessellate(shapes, pixels_per_point);
        let screen_descriptor: egui_wgpu::ScreenDescriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels,
            pixels_per_point,
        };
        let callback_command_buffers: Vec<wgpu::CommandBuffer> = self.renderer.update_buffers(
            device,
            queue,
            encoder,
            &paint_jobs,
            &screen_descriptor,
        );
        if !paint_jobs.is_empty() {
            let color_attachments: [Option<wgpu::RenderPassColorAttachment<'_>>; 1] =
                [Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })];
            let render_pass_descriptor: wgpu::RenderPassDescriptor<'_> =
                wgpu::RenderPassDescriptor {
                    label: Some("dogwood_user_interface_overlay"),
                    color_attachments: &color_attachments,
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                };
            let mut render_pass: wgpu::RenderPass<'static> = encoder
                .begin_render_pass(&render_pass_descriptor)
                .forget_lifetime();
            self.renderer
                .render(&mut render_pass, &paint_jobs, &screen_descriptor);
        }
        for texture_id in textures_delta.free.drain() {
            self.renderer.free_texture(&texture_id);
        }
        callback_command_buffers
    }

}
