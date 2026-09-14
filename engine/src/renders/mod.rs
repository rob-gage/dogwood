// Copyright Rob Gage 2026

mod scene_renderer;
mod user_interface_renderer;

fn create_render_shader_module(
    device: &wgpu::Device,
    label: &'static str,
    source: &'static str,
    file_path: &'static str,
) -> wgpu::ShaderModule {
    engine_compute::create_composed_shader_module(device, label, source, file_path)
}

pub use scene_renderer::SceneRenderer;
pub use user_interface_renderer::UserInterfaceRenderer;
