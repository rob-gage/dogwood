// Copyright Rob Gage 2026

use super::WindowError;

/// A native window managed by the host system
pub struct Window {
    /// The `winit` `Window`
    window: Option<winit::window::Window>,
    /// An error encountered while creating the native window
    error: Option<WindowError>,
}

impl Window {

    /// Opens the `HostWindow`
    pub fn open() -> Result<(), WindowError> {
        let event_loop: winit::event_loop::EventLoop<()> =
            winit::event_loop::EventLoop::new().map_err(WindowError::from)?;
        let mut host_window = Self { window: None, error: None, };
        event_loop.run_app(&mut host_window).map_err(WindowError::from)?;
        host_window.error.map_or(Ok(()), Err)
    }
}

impl winit::application::ApplicationHandler for Window {

    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if self.window.is_none() {
            match event_loop.create_window(winit::window::Window::default_attributes()) {
                Ok(window) => {
                    window.request_redraw();
                    self.window = Some(window);
                }
                Err(error) => {
                    self.error = Some(WindowError::from(error));
                    event_loop.exit();
                }
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        window_id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        let Some(window) = &self.window else { return };
        if window.id() != window_id { return; }
        match event {
            winit::event::WindowEvent::CloseRequested => event_loop.exit(),
            winit::event::WindowEvent::RedrawRequested => {}
            _ => {}
        }
    }

    fn suspended(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop)
    { self.window = None; }

}
