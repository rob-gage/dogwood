// Copyright Rob Gage 2026

use crate::rendering::{
    SceneRenderer,
    UserInterfaceRenderer,
};
use super::Game;
use std::{
    error::Error,
    sync::Arc,
};

/// A `winit` application used to run a `Game` implementor
pub struct GameApplication<G: Game> {
    /// The game run by this `GameApplication`
    game: G,
    /// The `winit` window used by this `GameApplication`
    window: Option<Arc<winit::window::Window>>,
    /// The graphics state used to render the window
    graphics: Option<Graphics>,
    /// The renderer for the active `Scene`
    scene_renderer: SceneRenderer,
    /// The renderer for the active user interface
    user_interface_renderer: UserInterfaceRenderer,
    /// An error with the `GameApplication`
    error: Option<Box<dyn Error>>,
}

impl<G: Game> GameApplication<G> {

    /// Creates a `GameApplication` for the supplied game
    pub fn new(game: G) -> Self {
        Self {
            game: game,
            window: None,
            graphics: None,
            scene_renderer: SceneRenderer::new(),
            user_interface_renderer: UserInterfaceRenderer::new(),
            error: None,
        }
    }

    /// Returns the error produced while running the application, if any
    pub fn finish(self) -> Result<(), Box<dyn Error>> { self.error.map_or(Ok(()), Err) }

    /// Renders the current game frame, with the user interface over the scene
    pub fn render(
        &mut self,
        command_encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
    ) {
        self.scene_renderer.render(self.game.scene(), command_encoder, target);
        self.user_interface_renderer.render(
            self.game.user_interface_context(),
            command_encoder,
            target,
        );
    }

    /// Acquires the next window frame, renders it, and presents it
    fn render_window(&mut self) {
        let frame = match self.graphics.as_ref().map(|graphics| graphics.surface.get_current_texture()) {
            None => return,
            Some(wgpu::CurrentSurfaceTexture::Success(frame))
            | Some(wgpu::CurrentSurfaceTexture::Suboptimal(frame)) => frame,
            Some(wgpu::CurrentSurfaceTexture::Outdated)
            | Some(wgpu::CurrentSurfaceTexture::Lost) => {
                let Some(graphics) = self.graphics.as_ref() else { return; };
                graphics.surface.configure(&graphics.device, &graphics.config);
                return;
            }
            Some(wgpu::CurrentSurfaceTexture::Timeout)
            | Some(wgpu::CurrentSurfaceTexture::Occluded)
            | Some(wgpu::CurrentSurfaceTexture::Validation) => return,
        };
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut command_encoder = {
            let Some(graphics) = self.graphics.as_ref() else { return; };
            graphics.device.create_command_encoder(
                &wgpu::CommandEncoderDescriptor { label: Some("Game frame") },
            )
        };
        self.render(&mut command_encoder, &view);
        let Some(graphics) = self.graphics.as_ref() else { return; };
        graphics.queue.submit(Some(command_encoder.finish()));
        graphics.queue.present(frame);
    }

    /// Reconfigures the graphics surface for a new window size
    fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 { return; }
        let Some(graphics) = self.graphics.as_mut() else { return; };
        graphics.config.width = width;
        graphics.config.height = height;
        graphics.surface.configure(&graphics.device, &graphics.config);
    }

}

struct Graphics {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
}

impl<G: Game> winit::application::ApplicationHandler for GameApplication<G> {

    /// Creates the application window and initializes its graphics resources
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if self.window.is_some() { return; }
        let attributes: winit::window::WindowAttributes =
            winit::window::WindowAttributes::default().with_title(G::TITLE);
        match event_loop.create_window(attributes) {
            Ok(window) => {
                let window: Arc<winit::window::Window> = Arc::new(window);
                let instance: wgpu::Instance = wgpu::Instance::default();
                let surface: wgpu::Surface = match instance.create_surface(window.clone()) {
                    Ok(surface) => surface,
                    Err(error) => {
                        self.error = Some(Box::new(error));
                        event_loop.exit();
                        return;
                    }
                };
                let adapter: wgpu::Adapter = match pollster::block_on(instance.request_adapter(
                    &wgpu::RequestAdapterOptions {
                        power_preference: wgpu::PowerPreference::default(),
                        compatible_surface: Some(&surface),
                        force_fallback_adapter: false,
                        apply_limit_buckets: false,
                    },
                )) {
                    Ok(adapter) => adapter,
                    Err(error) => {
                        self.error = Some(Box::new(error));
                        event_loop.exit();
                        return;
                    }
                };
                let (device, queue): (wgpu::Device, wgpu::Queue) =
                    match pollster::block_on(adapter.request_device(
                        &wgpu::DeviceDescriptor::default(),
                    )) {
                        Ok(result) => result,
                        Err(error) => {
                            self.error = Some(Box::new(error));
                            event_loop.exit();
                            return;
                        }
                    };
                let size: winit::dpi::PhysicalSize<u32> = window.inner_size();
                let Some(configuration): Option<wgpu::SurfaceConfiguration> =
                    surface.get_default_config(&adapter, size.width.max(1), size.height.max(1))
                else {
                    self.error = Some("The surface has no supported configuration".into());
                    event_loop.exit();
                    return;
                };
                surface.configure(&device, &configuration);
                self.graphics = Some(Graphics { surface, device, queue, config: configuration });
                self.window = Some(window);
            }
            Err(error) => {
                self.error = Some(Box::new(error));
                event_loop.exit();
            }
        }
    }

    /// Handles window lifecycle, resize, and redraw events
    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        use winit::event::WindowEvent::*;
        match event {
            CloseRequested => event_loop.exit(),
            Resized(size) => self.resize(size.width, size.height),
            RedrawRequested => self.render_window(),
            _ => {}
        }
    }

    /// Requests another redraw when the event loop is idle
    fn about_to_wait(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop) {
        if let Some(window) = self.window.as_ref() { window.request_redraw(); }
    }

}
