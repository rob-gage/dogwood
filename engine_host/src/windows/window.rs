// Copyright Rob Gage 2026

use super::WindowError;
use crate::system_events::{
    SystemKeyboardEvent,
    SystemMouseEvent,
    SystemWindowEvent,
};
use std::time::{
    Duration,
    Instant,
};

/// Fixed update frequency in updates per second
const UPDATES_PER_SECOND: u32 = 60;
/// Maximum number of fixed updates retained after a delayed event-loop iteration
const MAXIMUM_ACCUMULATED_UPDATES: u32 = 5;

/// A native window managed by the host system
pub struct Window<'handler> {
    /// Native Winit window
    window: Option<winit::window::Window>,
    /// Error encountered while creating the native window
    error: Option<WindowError>,
    /// Consumer of translated window events
    window_event_handler: &'handler mut dyn FnMut(SystemWindowEvent),
    /// Consumer of translated keyboard events
    keyboard_event_handler: &'handler mut dyn FnMut(SystemKeyboardEvent),
    /// Consumer of translated mouse events
    mouse_event_handler: &'handler mut dyn FnMut(SystemMouseEvent),
    /// Consumer of fixed-step game updates
    update_handler: &'handler mut dyn FnMut(Duration),
    /// Consumer of native redraw requests
    render_handler: &'handler mut dyn FnMut(),
    /// Time of the most recent scheduler iteration
    last_update_time: Instant,
    /// Elapsed time waiting for the next fixed-step update
    update_accumulator: Duration,
    /// Time of the next independent redraw request
    next_render_time: Instant,
}

impl<'handler> Window<'handler> {

    /// Opens a native window and dispatches its host events
    pub fn open(
        window_event_handler: &'handler mut dyn FnMut(SystemWindowEvent),
        keyboard_event_handler: &'handler mut dyn FnMut(SystemKeyboardEvent),
        mouse_event_handler: &'handler mut dyn FnMut(SystemMouseEvent),
        update_handler: &'handler mut dyn FnMut(Duration),
        render_handler: &'handler mut dyn FnMut(),
    ) -> Result<(), WindowError> {
        let event_loop: winit::event_loop::EventLoop<()> =
            winit::event_loop::EventLoop::new().map_err(WindowError::from)?;
        let mut window: Self = Self {
            window: None,
            error: None,
            window_event_handler,
            keyboard_event_handler,
            mouse_event_handler,
            update_handler,
            render_handler,
            last_update_time: Instant::now(),
            update_accumulator: Duration::ZERO,
            next_render_time: Instant::now(),
        };
        event_loop.run_app(&mut window).map_err(WindowError::from)?;
        window.error.map_or(Ok(()), Err)
    }

}

impl<'handler> winit::application::ApplicationHandler for Window<'handler> {

    /// Creates the native window when the event loop resumes
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if self.window.is_none() {
            let current_time: Instant = Instant::now();
            self.last_update_time = current_time;
            self.next_render_time = current_time;
            self.update_accumulator = Duration::ZERO;
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

    /// Translates and dispatches events for the managed window
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
                (self.window_event_handler)(host_event);
                event_loop.exit();
            }
            winit::event::WindowEvent::Resized(size) => {
                let host_event: SystemWindowEvent = SystemWindowEvent::Resized {
                    width: size.width,
                    height: size.height,
                };
                (self.window_event_handler)(host_event);
            }
            winit::event::WindowEvent::Focused(focused) => {
                let host_event: SystemWindowEvent = if focused { SystemWindowEvent::FocusAcquired }
                else { SystemWindowEvent::FocusReleased };
                (self.window_event_handler)(host_event);
            }
            winit::event::WindowEvent::KeyboardInput { event, .. } => {
                let host_event: SystemKeyboardEvent = SystemKeyboardEvent {
                    physical_key: event.physical_key,
                    pressed: event.state == winit::event::ElementState::Pressed,
                    repeat: event.repeat,
                };
                (self.keyboard_event_handler)(host_event);
            }
            winit::event::WindowEvent::MouseInput { state, button, .. } => {
                let host_event: SystemMouseEvent = SystemMouseEvent::Button {
                    button,
                    pressed: state == winit::event::ElementState::Pressed,
                };
                (self.mouse_event_handler)(host_event);
            }
            winit::event::WindowEvent::CursorMoved { position, .. } => {
                let host_event: SystemMouseEvent = SystemMouseEvent::CursorMoved {
                    x: position.x,
                    y: position.y,
                };
                (self.mouse_event_handler)(host_event);
            }
            winit::event::WindowEvent::CursorEntered { .. } => {
                let host_event: SystemMouseEvent = SystemMouseEvent::CursorEntered;
                (self.mouse_event_handler)(host_event);
            }
            winit::event::WindowEvent::CursorLeft { .. } => {
                let host_event: SystemMouseEvent = SystemMouseEvent::CursorLeft;
                (self.mouse_event_handler)(host_event);
            }
            winit::event::WindowEvent::MouseWheel { delta, .. } => match delta {
                winit::event::MouseScrollDelta::LineDelta(horizontal, vertical) => {
                    let host_event: SystemMouseEvent = SystemMouseEvent::WheelLines {
                        horizontal,
                        vertical,
                    };
                    (self.mouse_event_handler)(host_event);
                }
                winit::event::MouseScrollDelta::PixelDelta(position) => {
                    let host_event: SystemMouseEvent = SystemMouseEvent::WheelPixels {
                        horizontal: position.x,
                        vertical: position.y,
                    };
                    (self.mouse_event_handler)(host_event);
                }
            },
            winit::event::WindowEvent::RedrawRequested => (self.render_handler)(),
            _ => {}
        }
    }

    /// Releases the native window while the event loop is suspended
    fn suspended(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop) {
        self.window = None;
        self.update_accumulator = Duration::ZERO;
    }

    /// Runs bounded fixed-step updates and schedules the next event-loop wakeup
    fn about_to_wait(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if self.window.is_none() {
            event_loop.set_control_flow(winit::event_loop::ControlFlow::Wait);
            return;
        }
        let update_step: Duration = Duration::from_nanos(1_000_000_000 / UPDATES_PER_SECOND as u64);
        let maximum_accumulated_time: Duration = update_step * MAXIMUM_ACCUMULATED_UPDATES;
        let render_step: Duration = update_step;
        let current_time: Instant = Instant::now();
        let elapsed_time: Duration = current_time.duration_since(self.last_update_time);
        self.last_update_time = current_time;
        self.update_accumulator =
            self.update_accumulator.saturating_add(elapsed_time).min(maximum_accumulated_time);
        while self.update_accumulator >= update_step {
            (self.update_handler)(update_step);
            self.update_accumulator -= update_step;
        }
        let time_until_update: Duration = update_step - self.update_accumulator;
        let next_update_time: Instant = current_time + time_until_update;
        if current_time >= self.next_render_time {
            if let Some(window) = self.window.as_ref() { window.request_redraw(); }
            self.next_render_time = current_time + render_step;
        }
        let next_wakeup_time: Instant = next_update_time.min(self.next_render_time);
        event_loop.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(next_wakeup_time));
    }

}
