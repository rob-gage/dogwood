use super::scene_renderer::SceneRenderer;
use engine_compute::Accelerator;
use engine_graphics::{Color, MaterialAppearance};
use engine_physics::{
    materials::{Material, MaterialRegistry},
    scenes::Scene,
    simulation::SceneSimulationConfiguration,
};
use std::sync::Arc;

#[test]
fn test_scene_shader_builds_without_a_window_surface() {
    let accelerator: Arc<Accelerator> = Arc::new(Accelerator::new().unwrap());
    let mut materials: MaterialRegistry = MaterialRegistry::new();
    materials.register(Material::Gas {
        name: "Test Gas".into(),
        graphics: MaterialAppearance::from_color(Color::new_rgb(120, 160, 190)),
        density: 0.7,
        diffusivity: 0.1,
        extinction: 0.1,
        dissipation: 0.0,
        compressibility: 0.05,
    });
    let scene: Scene = Scene::new(
        &accelerator,
        materials,
        SceneSimulationConfiguration {
            gravity: [0.0, -1.0],
            ambient_temperature: 293.15,
            empty_space_thermal_conductivity: 0.0,
            empty_space_heat_capacity: 1.0,
            maximum_gas_concentration: 4.0,
            width: 1,
            height: 1,
            buffer_size: 2,
            streaming_batch_size: 1,
        },
    )
    .unwrap();
    let texture: wgpu::Texture =
        accelerator
            .wgpu_device()
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("scene shader test target"),
                size: wgpu::Extent3d {
                    width: 4,
                    height: 4,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
    let view: wgpu::TextureView = texture.create_view(&Default::default());
    let mut encoder: wgpu::CommandEncoder =
        accelerator
            .wgpu_device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("scene shader test"),
            });
    SceneRenderer::new().render(
        accelerator.as_ref(),
        Some(&scene),
        wgpu::TextureFormat::Rgba8Unorm,
        [0, 0, 4, 4],
        [0.0; 2],
        [1.0; 2],
        true,
        0,
        false,
        false,
        &mut encoder,
        &view,
    );
    accelerator.wgpu_queue().submit(Some(encoder.finish()));
}
