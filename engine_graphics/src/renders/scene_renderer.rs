// Copyright Rob Gage 2026

use super::{
    scene_renderer_cellular_pass::SceneRendererCellularPass,
    scene_renderer_fluid_pass::SceneRendererFluidPass,
    scene_renderer_gas_pass::SceneRendererGasPass,
    scene_renderer_overlay_pass::SceneRendererOverlayPass,
    scene_renderer_pressure_pass::SceneRendererPressurePass,
    scene_renderer_sprites_pass::SceneRendererSpritesPass,
};
use crate::world::scenes::{
    SceneGraphics,
    SceneGraphicsCellular,
    SceneGraphicsFluid,
    SceneGraphicsGas,
    SceneGraphicsPressure,
    SceneGraphicsSprites,
};
use engine_host::acceleration::Accelerator;

/// Appearance drawn by a scene renderer
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub enum SceneRendererViewMode {
    /// Draws the scene's appearance
    #[default]
    Graphics,
    /// Also tints every resident cell by its pressure
    Pressure,
}

/// Draws the unlit appearance of a scene into one view's texture
pub struct SceneRenderer {
    /// Appearance drawn by this renderer
    view_mode: SceneRendererViewMode,
    /// Separate background rendering, sharing sprite batching and alpha blending
    parallax_pass: SceneRendererSpritesPass,
    /// Renderer of sprites with negative depth, drawn behind cellular content
    behind_cellular_sprites_pass: SceneRendererSpritesPass,
    /// Renderer of resident fluid particles rasterized at cell resolution
    fluid_pass: SceneRendererFluidPass,
    /// Renderer of resident gas cells
    gas_pass: SceneRendererGasPass,
    /// Renderer of resident pressure cells
    pressure_pass: SceneRendererPressurePass,
    /// Renderer of cell-aligned overlays drawn over every scene view
    overlay_pass: SceneRendererOverlayPass,
    /// Renderer of resident cellular tiles
    cellular_pass: SceneRendererCellularPass,
    /// Renderer of sprites with nonnegative depth, drawn above cellular content
    above_cellular_sprites_pass: SceneRendererSpritesPass,
}

impl SceneRenderer {

    /// Creates the scene rendering resources
    pub fn new(accelerator: &Accelerator) -> Self {
        Self {
            view_mode: SceneRendererViewMode::Graphics,
            parallax_pass: SceneRendererSpritesPass::new(accelerator, i32::MIN..=i32::MAX),
            behind_cellular_sprites_pass: SceneRendererSpritesPass::new(accelerator, i32::MIN..=-1),
            fluid_pass: SceneRendererFluidPass::new(accelerator),
            gas_pass: SceneRendererGasPass::new(accelerator),
            pressure_pass: SceneRendererPressurePass::new(accelerator),
            overlay_pass: SceneRendererOverlayPass::new(accelerator),
            cellular_pass: SceneRendererCellularPass::new(accelerator),
            above_cellular_sprites_pass: SceneRendererSpritesPass::new(accelerator, 0..=i32::MAX),
        }
    }

    /// Sets the appearance drawn by this renderer
    pub fn set_view_mode(&mut self, view_mode: SceneRendererViewMode) {
        self.view_mode = view_mode;
    }

    /// Returns the color the pressure view mode gives a cell at an absolute pressure in
    /// atmospheres, matching the shader
    pub fn pressure_color(pressure: f32) -> [f32; 4] {
        let (red, green, blue, alpha): (f32, f32, f32, f32) = if pressure < 1.0 {
            let t: f32 = ((1.0 - pressure) / 0.5).clamp(0.0, 1.0);
            let amount: f32 = (1.0 - pressure).clamp(0.0, 1.0);
            (
                0.55 + (0.05 - 0.55) * amount,
                0.75 + (0.15 - 0.75) * amount,
                1.0 + (0.8 - 1.0) * amount,
                0.85 * t.sqrt(),
            )
        } else {
            let t: f32 = ((pressure - 1.0) / 0.5).clamp(0.0, 1.0);
            let scale: f32 = (pressure.ln() / 4095.0_f32.ln()).clamp(0.0, 1.0);
            let stops: [(f32, [f32; 3]); 5] = [
                (0.0, [1.0, 0.9, 0.2]),
                (0.25, [1.0, 0.5, 0.1]),
                (0.5, [0.9, 0.1, 0.1]),
                (0.75, [0.8, 0.1, 0.6]),
                (1.0, [1.0, 1.0, 1.0]),
            ];
            let stop_index: usize = ((scale * 4.0).floor() as usize).min(3);
            let (start, start_color): (f32, [f32; 3]) = stops[stop_index];
            let (end, end_color): (f32, [f32; 3]) = stops[stop_index + 1];
            let amount: f32 = (scale - start) / (end - start);
            (
                start_color[0] + (end_color[0] - start_color[0]) * amount,
                start_color[1] + (end_color[1] - start_color[1]) * amount,
                start_color[2] + (end_color[2] - start_color[2]) * amount,
                0.85 * t.sqrt(),
            )
        };
        [red, green, blue, alpha]
    }

    /// Draws a scene's visible area letterboxed into a texture and submits the work
    pub fn render(
        &mut self,
        accelerator: &Accelerator,
        texture_view: &wgpu::TextureView,
        graphics: &SceneGraphics<'_>,
    ) {
        let texture: &wgpu::Texture = texture_view.texture();
        let format: wgpu::TextureFormat = texture.format();
        let texture_width: u32 = texture.width();
        let texture_height: u32 = texture.height();
        let visible_width: f32 = graphics.visible_width;
        let visible_height: f32 = graphics.visible_height;
        let pixels_per_tile: f32 = if visible_width > 0.0 && visible_height > 0.0 {
            (texture_width as f32 / visible_width).min(texture_height as f32 / visible_height)
        } else {
            0.0
        };
        let viewport_width: u32 =
            ((visible_width * pixels_per_tile).round() as u32).min(texture_width);
        let viewport_height: u32 =
            ((visible_height * pixels_per_tile).round() as u32).min(texture_height);
        let viewport_x: u32 = (texture_width - viewport_width) / 2;
        let viewport_y: u32 = (texture_height - viewport_height) / 2;
        let is_viewport_visible: bool =
            viewport_width > 0 && viewport_height > 0 && pixels_per_tile > 0.0;
        // draw cellular content only when the viewport and its resident region are nonempty
        let cellular_graphics: Option<&SceneGraphicsCellular<'_>> =
            graphics.cellular.as_ref().filter(|cellular_graphics| {
                is_viewport_visible
                    && cellular_graphics.region_width > 0
                    && cellular_graphics.region_height > 0
                    && cellular_graphics.cellular_tile_size > 0
            });
        let fluid_graphics: Option<&SceneGraphicsFluid<'_>> =
            graphics.fluid.as_ref().filter(|fluid_graphics| {
                is_viewport_visible
                    && fluid_graphics.region_width > 0
                    && fluid_graphics.region_height > 0
                    && fluid_graphics.cellular_tile_size > 0
            });
        let gas_graphics: Option<&SceneGraphicsGas<'_>> =
            graphics.gas.as_ref().filter(|gas_graphics| {
                is_viewport_visible
                    && gas_graphics.region_width > 0
                    && gas_graphics.region_height > 0
                    && gas_graphics.cellular_tile_size > 0
            });
        let pressure_graphics: Option<&SceneGraphicsPressure<'_>> =
            graphics.pressure.as_ref().filter(|pressure_graphics| {
                self.view_mode == SceneRendererViewMode::Pressure
                    && is_viewport_visible
                    && pressure_graphics.region_width > 0
                    && pressure_graphics.region_height > 0
                    && pressure_graphics.cellular_tile_size > 0
            });
        if is_viewport_visible {
            // Physical distance sorts first, then sprite order within coplanar backgrounds.
            let mut layers: Vec<_> = graphics.parallax.iter()
                .flat_map(|layer| layer.visible_copies(visible_width, visible_height)).collect();
            layers.sort_by(|a, b| b.depth.total_cmp(&a.depth).then(a.order.cmp(&b.order)));
            let backgrounds = SceneGraphicsSprites {
                sprites: layers.iter().map(|layer| (
                    layer.texture, layer.center[0], layer.center[1],
                    layer.size[0], layer.size[1], layer.rotation, 0,
                    layer.texture_region, layer.color,
                )).collect(),
            };
            self.parallax_pass.prepare(
                accelerator, format, viewport_x, viewport_y,
                viewport_width, viewport_height, pixels_per_tile, &backgrounds,
            );
            self.behind_cellular_sprites_pass.prepare(
                accelerator,
                format,
                viewport_x,
                viewport_y,
                viewport_width,
                viewport_height,
                pixels_per_tile,
                &graphics.sprites,
            );
        }
        if let Some(cellular_graphics) = cellular_graphics {
            self.cellular_pass.prepare(
                accelerator,
                format,
                viewport_x,
                viewport_y,
                viewport_width,
                viewport_height,
                pixels_per_tile,
                cellular_graphics,
            );
        }
        if let Some(fluid_graphics) = fluid_graphics {
            self.fluid_pass.prepare(
                accelerator,
                format,
                viewport_x,
                viewport_y,
                viewport_width,
                viewport_height,
                pixels_per_tile,
                fluid_graphics,
            );
        }
        if let Some(gas_graphics) = gas_graphics {
            self.gas_pass.prepare(
                accelerator,
                format,
                viewport_x,
                viewport_y,
                viewport_width,
                viewport_height,
                pixels_per_tile,
                gas_graphics,
            );
        }
        if let Some(pressure_graphics) = pressure_graphics {
            self.pressure_pass.prepare(
                accelerator,
                format,
                viewport_x,
                viewport_y,
                viewport_width,
                viewport_height,
                pixels_per_tile,
                pressure_graphics,
            );
        }
        if is_viewport_visible {
            self.above_cellular_sprites_pass.prepare(
                accelerator,
                format,
                viewport_x,
                viewport_y,
                viewport_width,
                viewport_height,
                pixels_per_tile,
                &graphics.sprites,
            );
            self.overlay_pass.prepare(
                accelerator,
                format,
                viewport_x,
                viewport_y,
                viewport_width,
                viewport_height,
                pixels_per_tile,
                visible_width,
                visible_height,
                graphics.cellular_tile_size,
                graphics.cell_grid_origin,
                &graphics.overlays,
            );
        }
        let mut command_encoder: wgpu::CommandEncoder = accelerator.device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("`SceneRenderer::render`"),
            });
        if fluid_graphics.is_some() {
            self.fluid_pass.shade(&mut command_encoder);
        }
        let mut render_pass: wgpu::RenderPass<'_> =
            command_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("`SceneRenderer::render`"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: texture_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.0, g: 0.0, b: 0.0, a: 1.0 }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
        if is_viewport_visible {
            render_pass.set_viewport(
                viewport_x as f32,
                viewport_y as f32,
                viewport_width as f32,
                viewport_height as f32,
                0.0,
                1.0,
            );
            render_pass.set_scissor_rect(viewport_x, viewport_y, viewport_width, viewport_height);
            self.parallax_pass.draw(&mut render_pass, format);
            self.behind_cellular_sprites_pass.draw(&mut render_pass, format);
            if fluid_graphics.is_some() {
                self.fluid_pass.draw(&mut render_pass, format);
            }
            if gas_graphics.is_some() {
                self.gas_pass.draw(&mut render_pass, format);
            }
            if cellular_graphics.is_some() {
                self.cellular_pass.draw(&mut render_pass, format);
            }
            self.above_cellular_sprites_pass.draw(&mut render_pass, format);
            if pressure_graphics.is_some() {
                self.pressure_pass.draw(&mut render_pass, format);
            }
            self.overlay_pass.draw(&mut render_pass, format);
        }
        drop(render_pass);
        accelerator.queue().submit([command_encoder.finish()]);
    }

}
