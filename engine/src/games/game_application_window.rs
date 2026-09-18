// Copyright Rob Gage 2026

use super::GameApplication;
use crate::games::Game;
use std::sync::Arc;

impl<G: Game> GameApplication<G> {
    /// Handles window lifecycle, resize, and redraw events
    pub fn handle_window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        event: &winit::event::WindowEvent,
    ) -> bool {
        use winit::event::WindowEvent::*;
        let ui_consumed: bool = self.window.as_ref().is_some_and(|window| {
            self.game
                .user_interface_context()
                .process_window_event(window, event)
        });
        self.handle_pointer_event(event, ui_consumed);
        match event {
            CloseRequested => event_loop.exit(),
            Resized(size) => self.resize(size.width, size.height),
            KeyboardInput { event, .. } if !ui_consumed => {
                self.keyboard_input_state.process_event(event)
            }
            RedrawRequested => self.render(),
            _ => {}
        }
        ui_consumed
    }

    /// Requests another redraw when the event loop is idle
    pub fn redraw(&self) {
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    /// Creates the application window and initializes its graphics resources
    pub fn window_initialize(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attributes: winit::window::WindowAttributes =
            winit::window::WindowAttributes::default().with_title(&self.title);
        match event_loop.create_window(attributes) {
            Ok(window) => {
                let window: Arc<winit::window::Window> = Arc::new(window);
                let surface: wgpu::Surface = match self
                    .accelerator
                    .wgpu_instance()
                    .create_surface(window.clone())
                {
                    Ok(surface) => surface,
                    Err(error) => {
                        tracing::error!(%error, "failed to create rendering surface");
                        self.error = Some(Box::new(error));
                        event_loop.exit();
                        return;
                    }
                };
                let size: winit::dpi::PhysicalSize<u32> = window.inner_size();
                let Some(configuration) = surface.get_default_config(
                    self.accelerator.wgpu_adapter(),
                    size.width.max(1),
                    size.height.max(1),
                ) else {
                    tracing::error!("rendering surface has no supported configuration");
                    self.error = Some("The surface has no supported configuration".into());
                    event_loop.exit();
                    return;
                };
                surface.configure(self.accelerator.wgpu_device(), &configuration);
                self.game.user_interface_context().initialize_window(
                    window.as_ref(),
                    self.accelerator
                        .wgpu_device()
                        .limits()
                        .max_texture_dimension_2d as usize,
                );
                self.surface = Some(surface);
                self.surface_configuration = Some(configuration);
                self.update_camera(0.0);
                self.window = Some(window);
                tracing::info!(title = %self.title, "application window initialized");
            }
            Err(error) => {
                tracing::error!(%error, "failed to create application window");
                self.error = Some(Box::new(error));
                event_loop.exit();
            }
        }
    }

    /// Centers an aspect-constrained viewport within editor content bounds
    pub(super) fn fit_aspect_ratio(bounds: [u32; 4], aspect_ratio: f32) -> [u32; 4] {
        let [x, y, width, height] = bounds;
        if width as f32 / height as f32 > aspect_ratio {
            let viewport_width: u32 = (height as f32 * aspect_ratio).round() as u32;
            [x + (width - viewport_width) / 2, y, viewport_width, height]
        } else {
            let viewport_height: u32 = (width as f32 / aspect_ratio).round() as u32;
            [x, y, width, viewport_height]
        }
    }

    /// Returns the exact physical viewport used by scene rendering
    pub(super) fn scene_viewport(&self, configuration: &wgpu::SurfaceConfiguration) -> [u32; 4] {
        self.scene_viewport_bounds.map_or_else(
            || [0, 0, configuration.width, configuration.height],
            |bounds| {
                let x: u32 = bounds[0].min(configuration.width - 1);
                let y: u32 = bounds[1].min(configuration.height - 1);
                Self::fit_aspect_ratio(
                    [
                        x,
                        y,
                        bounds[2].min(configuration.width - x).max(1),
                        bounds[3].min(configuration.height - y).max(1),
                    ],
                    self.camera.width / self.camera.height,
                )
            },
        )
    }

    /// Returns the exact world-space camera size used by scene rendering
    pub(super) fn scene_camera_size(&self, viewport: [u32; 4]) -> [f32; 2] {
        let mut camera_size: [f32; 2] = [
            self.camera.width * self.camera.zoom,
            self.camera.height * self.camera.zoom,
        ];
        let viewport_aspect: f32 = viewport[2] as f32 / viewport[3] as f32;
        let camera_aspect: f32 = camera_size[0] / camera_size[1];
        if viewport_aspect > camera_aspect {
            camera_size[0] = camera_size[1] * viewport_aspect;
        } else {
            camera_size[1] = camera_size[0] / viewport_aspect;
        }
        camera_size
    }
}
