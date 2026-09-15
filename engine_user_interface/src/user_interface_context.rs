// Copyright Rob Gage 2026

use crate::UserInterface;
use crate::Widget;
use std::cell::RefCell;

/// The user interface displayed by a `Game`
pub struct UserInterfaceContext {
    egui_context: egui::Context,
    window_state: RefCell<Option<egui_winit::State>>,
    output: RefCell<egui::FullOutput>,
}

impl UserInterfaceContext {
    /// Creates an empty `UserInterfaceContext`
    pub fn new() -> Self {
        let egui_context: egui::Context = egui::Context::default();
        egui_context.set_fonts(egui::FontDefinitions::default());
        Self {
            egui_context,
            window_state: RefCell::new(None),
            output: RefCell::new(egui::FullOutput::default()),
        }
    }

    /// Runs one user-interface frame
    pub fn run(
        &self,
        input: egui::RawInput,
        add_contents: impl FnOnce(&mut UserInterface),
    ) -> egui::FullOutput {
        let mut add_contents: Option<_> = Some(add_contents);
        let output: egui::FullOutput = self.egui_context.run_ui(input, |ui| {
            let mut ui: UserInterface = UserInterface(ui);
            if let Some(add_contents) = add_contents.take() {
                add_contents(&mut ui);
            }
        });
        let mut returned_output: egui::FullOutput = output.clone();
        returned_output.textures_delta.clear();
        *self.output.borrow_mut() = output;
        returned_output
    }

    /// Displays a widget in the next user-interface frame.
    pub fn add_widget(
        &self,
        widget: &mut impl Widget,
        size: [u32; 2],
        window: &egui_winit::winit::window::Window,
    ) {
        let input: egui::RawInput = self
            .window_state
            .borrow_mut()
            .as_mut()
            .map(|state| state.take_egui_input(window))
            .unwrap_or_else(|| egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(size[0] as f32, size[1] as f32),
                )),
                ..Default::default()
            });
        let output: egui::FullOutput = self.run(input, |ui| {
            ui.add_widget(widget);
        });
        if let Some(state) = self.window_state.borrow_mut().as_mut() {
            state.handle_platform_output(window, output.platform_output);
        }
    }

    /// Initializes pointer and window input handling for this user interface
    pub fn initialize_window(
        &self,
        window: &egui_winit::winit::window::Window,
        max_texture_side: usize,
    ) {
        *self.window_state.borrow_mut() = Some(egui_winit::State::new(
            self.egui_context.clone(),
            egui::ViewportId::ROOT,
            window,
            Some(window.scale_factor() as f32),
            window.theme(),
            Some(max_texture_side),
        ));
    }

    /// Passes a window event to egui and returns whether egui consumed it
    pub fn process_window_event(
        &self,
        window: &egui_winit::winit::window::Window,
        event: &egui_winit::winit::event::WindowEvent,
    ) -> bool {
        self.window_state
            .borrow_mut()
            .as_mut()
            .is_some_and(|state| state.on_window_event(window, event).consumed)
    }

    /// Takes the latest UI output for rendering.
    pub fn take_output(&self) -> egui::FullOutput {
        std::mem::take(&mut *self.output.borrow_mut())
    }

    /// Returns a reference to this `UserInterfaceContext`'s `egui::Context`
    pub fn egui_context(&self) -> &egui::Context {
        &self.egui_context
    }
}

impl Drop for UserInterfaceContext {
    fn drop(&mut self) {
        self.output.get_mut().textures_delta.clear();
    }
}
