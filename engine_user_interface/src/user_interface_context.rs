// Copyright Rob Gage 2026

use crate::UserInterface;
use crate::Widget;
use std::cell::RefCell;

/// The user interface displayed by a `Game`
pub struct UserInterfaceContext {
    egui_context: egui::Context,
    output: RefCell<egui::FullOutput>,
}

impl UserInterfaceContext {

    /// Creates an empty `UserInterfaceContext`
    pub fn new() -> Self {
        let egui_context = egui::Context::default();
        egui_context.set_fonts(egui::FontDefinitions::default());
        Self {
            egui_context,
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
        let output = self.egui_context.run_ui(input, |ui| {
            let mut ui: UserInterface = UserInterface(ui);
            if let Some(add_contents) = add_contents.take() { add_contents(&mut ui); }
        });
        let mut returned_output = output.clone();
        returned_output.textures_delta.clear();
        *self.output.borrow_mut() = output;
        returned_output
    }

    /// Displays a widget in the next user-interface frame.
    pub fn add_widget(&self, widget: &mut impl Widget, size: [u32; 2]) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(size[0] as f32, size[1] as f32),
            )),
            ..Default::default()
        };
        self.run(input, |ui| {
            ui.add_widget(widget);
        });
    }

    /// Takes the latest UI output for rendering.
    pub fn take_output(&self) -> egui::FullOutput {
        std::mem::take(&mut *self.output.borrow_mut())
    }

    /// Returns a reference to this `UserInterfaceContext`'s `egui::Context`
    pub fn egui_context(&self) -> &egui::Context { &self.egui_context }

}

impl Drop for UserInterfaceContext {

    fn drop(&mut self) {
        self.output.get_mut().textures_delta.clear();
    }

}
