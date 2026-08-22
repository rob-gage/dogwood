// Copyright Rob Gage 2026

use engine_compute::Accelerator;

/// Graphics-specific state used to render to a window surface.
pub struct RenderContext {
    surface: wgpu::Surface<'static>,
    configuration: wgpu::SurfaceConfiguration,
}

impl RenderContext {

    /// Creates and configures a `RenderContext` for a window surface.
    pub fn new(
        surface: wgpu::Surface<'static>,
        accelerator: &Accelerator,
        width: u32,
        height: u32,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let Some(configuration) = surface.get_default_config(
            accelerator.wgpu_adapter(),
            width.max(1),
            height.max(1),
        ) else {
            return Err("The surface has no supported configuration".into());
        };
        surface.configure(accelerator.wgpu_device(), &configuration);

        Ok(Self { surface, configuration })
    }

    /// Returns the window surface used by this `RenderContext`.
    pub const fn surface(&self) -> &wgpu::Surface<'static> { &self.surface }

    /// Returns the current surface configuration.
    pub const fn configuration(&self) -> &wgpu::SurfaceConfiguration { &self.configuration }

    /// Reconfigures the surface using the current configuration.
    pub fn configure(&self, accelerator: &Accelerator) {
        self.surface.configure(accelerator.wgpu_device(), &self.configuration);
    }

    /// Updates the surface dimensions and reconfigures it.
    pub fn resize(&mut self, accelerator: &Accelerator, width: u32, height: u32) {
        if width == 0 || height == 0 { return; }
        self.configuration.width = width;
        self.configuration.height = height;
        self.configure(accelerator);
    }

}
