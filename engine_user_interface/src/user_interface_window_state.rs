// Copyright Rob Gage 2026

use crate::UserInterfaceContext;
use egui_winit::EventResponse;
use winit::{
    event::WindowEvent,
    window::Window,
};

/// Owns egui-winit input state for one native window
pub struct UserInterfaceWindowState {
    egui_winit_state: egui_winit::State,
}

impl UserInterfaceWindowState {

    /// Creates input state bound to the root viewport of a user interface context
    pub fn new(
        context: &UserInterfaceContext,
        window: &Window,
        max_texture_side: Option<usize>,
    ) -> Self {
        let egui_winit_state: egui_winit::State = egui_winit::State::new(
            context.egui_context().clone(),
            egui::ViewportId::ROOT,
            window,
            Some(window.scale_factor() as f32),
            window.theme(),
            max_texture_side,
        );
        Self { egui_winit_state }
    }

    /// Forwards native events and returns advisory consumption without changing gameplay routing
    ///
    /// Lifecycle events remain unconsumed and a redraw event never requests another redraw
    pub fn process_window_event(
        &mut self,
        window: &Window,
        event: &WindowEvent,
    ) -> EventResponse {
        let mut event_response: EventResponse = self.egui_winit_state.on_window_event(window, event);
        if matches!(
            event,
            WindowEvent::ActivationTokenDone { .. }
                | WindowEvent::Resized(_)
                | WindowEvent::Moved(_)
                | WindowEvent::CloseRequested
                | WindowEvent::Destroyed
                | WindowEvent::Focused(_)
                | WindowEvent::ScaleFactorChanged { .. }
                | WindowEvent::ThemeChanged(_)
                | WindowEvent::Occluded(_)
                | WindowEvent::RedrawRequested
        ) {
            event_response.consumed = false;
        }
        if matches!(event, WindowEvent::RedrawRequested) {
            event_response.repaint = false;
        }
        event_response
    }

    /// Takes accumulated egui input and refreshes its time, viewport, and scale information
    pub fn take_input(&mut self, window: &Window) -> egui::RawInput {
        self.egui_winit_state.take_egui_input(window)
    }

    /// Applies egui clipboard, cursor, URL, and IME output to the native window
    pub fn handle_platform_output(
        &mut self,
        window: &Window,
        output: egui::PlatformOutput,
    ) {
        self.egui_winit_state.handle_platform_output(window, output);
    }

}
