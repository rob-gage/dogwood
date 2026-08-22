// Copyright Rob Gage 2026

use crate::UserInterface;

/// The user interface displayed by a `Game`
pub struct UserInterfaceContext {
    egui_context: egui::Context,
}

impl UserInterfaceContext {

    /// Creates an empty `UserInterfaceContext`
    pub fn new() -> Self { Self { egui_context: egui::Context::default(), } }

    /// Runs one user-interface frame
    pub fn run(
        &self,
        input: egui::RawInput,
        add_contents: impl FnOnce(&mut UserInterface),
    ) -> egui::FullOutput {
        let mut add_contents: Option<_> = Some(add_contents);
        self.egui_context.run_ui(input, |ui| {
            let mut ui: UserInterface = UserInterface(ui);
            if let Some(add_contents) = add_contents.take() { add_contents(&mut ui); }
        })
    }

    /// Returns a reference to this `UserInterfaceContext`'s `egui::Context`
    pub(crate) fn egui_context(&self) -> &egui::Context { &self.egui_context }

}
