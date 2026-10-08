// Copyright Rob Gage 2026

use super::WindowError;
use crate::system_events::{
    SystemEventHandler,
    SystemKeyboardEvent,
    SystemMouseEvent,
    SystemWindowEvent,
};

/// A native window managed by the host system
pub struct Window<'handler, H: SystemEventHandler> {
    /// Native Winit window
    window: Option<winit::window::Window>,
    /// Error encountered while creating the native window
    error: Option<WindowError>,
    /// Consumer of translated host system_events
    handler: &'handler mut H,
}

impl<'handler, H: SystemEventHandler> Window<'handler, H> {

    /// Opens a native window and dispatches its host system_events
    pub fn open(handler: &'handler mut H) -> Result<(), WindowError> {
        let event_loop: winit::event_loop::EventLoop<()> =
            winit::event_loop::EventLoop::new().map_err(WindowError::from)?;
        let mut window: Self = Self {
            window: None,
            error: None,
            handler,
        };
        event_loop.run_app(&mut window).map_err(WindowError::from)?;
        window.error.map_or(Ok(()), Err)
    }

}

impl<'handler, H: SystemEventHandler> winit::application::ApplicationHandler for Window<'handler, H> {

    /// Creates the native window when the event loop resumes
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

    /// Translates and dispatches system_events for the managed window
    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        window_id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        let is_managed_window: bool = self
            .window
            .as_ref()
            .is_some_and(|window| window.id() == window_id);
        if !is_managed_window {
            return;
        }
        match event {
            winit::event::WindowEvent::CloseRequested => {
                let host_event: SystemWindowEvent = SystemWindowEvent::CloseRequested;
                self.handler.window_event(host_event);
                event_loop.exit();
            }
            winit::event::WindowEvent::Resized(size) => {
                let host_event: SystemWindowEvent = SystemWindowEvent::Resized {
                    width: size.width,
                    height: size.height,
                };
                self.handler.window_event(host_event);
            }
            winit::event::WindowEvent::Focused(focused) => {
                let host_event: SystemWindowEvent = if focused { SystemWindowEvent::FocusAcquired }
                else { SystemWindowEvent::FocusReleased };
                self.handler.window_event(host_event);
            }
            winit::event::WindowEvent::KeyboardInput { event, .. } => {
                let host_event: SystemKeyboardEvent = SystemKeyboardEvent {
                    physical_key: event.physical_key,
                    pressed: event.state == winit::event::ElementState::Pressed,
                    repeat: event.repeat,
                };
                self.handler.keyboard_event(host_event);
            }
            winit::event::WindowEvent::MouseInput { state, button, .. } => {
                let host_event: SystemMouseEvent = SystemMouseEvent::Button {
                    button,
                    pressed: state == winit::event::ElementState::Pressed,
                };
                self.handler.mouse_event(host_event);
            }
            winit::event::WindowEvent::CursorMoved { position, .. } => {
                let host_event: SystemMouseEvent = SystemMouseEvent::CursorMoved {
                    x: position.x,
                    y: position.y,
                };
                self.handler.mouse_event(host_event);
            }
            winit::event::WindowEvent::CursorEntered { .. } => {
                let host_event: SystemMouseEvent = SystemMouseEvent::CursorEntered;
                self.handler.mouse_event(host_event);
            }
            winit::event::WindowEvent::CursorLeft { .. } => {
                let host_event: SystemMouseEvent = SystemMouseEvent::CursorLeft;
                self.handler.mouse_event(host_event);
            }
            winit::event::WindowEvent::MouseWheel { delta, .. } => match delta {
                winit::event::MouseScrollDelta::LineDelta(horizontal, vertical) => {
                    let host_event: SystemMouseEvent = SystemMouseEvent::WheelLines {
                        horizontal,
                        vertical,
                    };
                    self.handler.mouse_event(host_event);
                }
                winit::event::MouseScrollDelta::PixelDelta(position) => {
                    let host_event: SystemMouseEvent = SystemMouseEvent::WheelPixels {
                        horizontal: position.x,
                        vertical: position.y,
                    };
                    self.handler.mouse_event(host_event);
                }
            },
            winit::event::WindowEvent::RedrawRequested => {}
            _ => {}
        }
    }

    /// Releases the native window while the event loop is suspended
    fn suspended(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop) {
        self.window = None;
    }

}
