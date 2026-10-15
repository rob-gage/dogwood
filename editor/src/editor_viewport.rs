// Copyright Rob Gage 2026

use engine::{
    scenes::ScenePosition,
    user_interface::{
        egui,
        wgpu,
        UserInterfaceRenderer,
    },
    Accelerator,
};

/// Offscreen texture the hosted game is drawn into, shown at the size of the viewport tab
pub struct EditorViewport {
    /// View of the texture the game is drawn into
    texture_view: wgpu::TextureView,
    /// Identifier of the texture registered with the editor's user interface renderer
    texture_id: egui::TextureId,
}

impl EditorViewport {

    /// Creates a viewport texture with physical pixel dimensions and registers it for display
    pub fn new(
        accelerator: &Accelerator,
        user_interface_renderer: &mut UserInterfaceRenderer,
        width: u32,
        height: u32,
    ) -> Self {
        let texture_view: wgpu::TextureView = Self::texture_view_new(accelerator, width, height);
        let texture_id: egui::TextureId = user_interface_renderer.renderer_mut()
            .register_native_texture(
                accelerator.device(),
                &texture_view,
                wgpu::FilterMode::Nearest,
            );
        Self { texture_view, texture_id }
    }

    /// Replaces the viewport texture when its physical pixel dimensions change
    pub fn resize(
        &mut self,
        accelerator: &Accelerator,
        user_interface_renderer: &mut UserInterfaceRenderer,
        width: u32,
        height: u32,
    ) {
        let texture: &wgpu::Texture = self.texture_view.texture();
        if texture.width() == width && texture.height() == height { return; }
        self.texture_view = Self::texture_view_new(accelerator, width, height);
        user_interface_renderer.renderer_mut().update_egui_texture_from_wgpu_texture(
            accelerator.device(),
            &self.texture_view,
            wgpu::FilterMode::Nearest,
            self.texture_id,
        );
    }

    /// Returns the view of the texture the game is drawn into
    pub fn texture_view(&self) -> &wgpu::TextureView { &self.texture_view }

    /// Returns the identifier of the texture registered for display
    pub fn texture_id(&self) -> egui::TextureId { self.texture_id }

    /// Returns the scene position under a viewport pixel seen through a camera
    pub fn scene_position(
        &self,
        camera_position: ScenePosition,
        visible_width: f32,
        visible_height: f32,
        pixel_x: f32,
        pixel_y: f32,
    ) -> ScenePosition {
        let pixels_per_tile: f32 = self.pixels_per_tile(visible_width, visible_height);
        if pixels_per_tile <= 0.0 { return camera_position; }
        let texture: &wgpu::Texture = self.texture_view.texture();
        // scene rendering centers the camera and draws tiles upward
        camera_position.translate(
            f64::from((pixel_x - texture.width() as f32 / 2.0) / pixels_per_tile),
            f64::from((texture.height() as f32 / 2.0 - pixel_y) / pixels_per_tile),
        )
    }

    /// Returns the viewport pixel showing a scene position seen through a camera
    pub fn pixel_position(
        &self,
        camera_position: ScenePosition,
        visible_width: f32,
        visible_height: f32,
        position: ScenePosition,
    ) -> (f32, f32) {
        let pixels_per_tile: f32 = self.pixels_per_tile(visible_width, visible_height);
        let texture: &wgpu::Texture = self.texture_view.texture();
        let (x, y): (f64, f64) = position.relative_position(camera_position);
        (
            texture.width() as f32 / 2.0 + x as f32 * pixels_per_tile,
            texture.height() as f32 / 2.0 - y as f32 * pixels_per_tile,
        )
    }

    /// Returns the pixels per tile of a camera's visible area letterboxed into the texture
    pub fn pixels_per_tile(&self, visible_width: f32, visible_height: f32) -> f32 {
        if visible_width <= 0.0 || visible_height <= 0.0 { return 0.0; }
        let texture: &wgpu::Texture = self.texture_view.texture();
        (texture.width() as f32 / visible_width).min(texture.height() as f32 / visible_height)
    }

    /// Creates a texture the scene and game user interface can draw into and egui can sample
    fn texture_view_new(accelerator: &Accelerator, width: u32, height: u32) -> wgpu::TextureView {
        accelerator.device()
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("`EditorViewport.texture_view`"),
                size: wgpu::Extent3d {
                    width: width.max(1),
                    height: height.max(1),
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                // scene rendering writes linear color that this format encodes as sRGB
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&wgpu::TextureViewDescriptor::default())
    }

}
