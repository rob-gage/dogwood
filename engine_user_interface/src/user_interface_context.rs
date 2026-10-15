// Copyright Rob Gage 2026

use crate::{
    UserInterface,
    UserInterfaceTheme,
};

/// Owns persistent egui composition state for one user interface
#[derive(Clone)]
pub struct UserInterfaceContext {
    egui_context: egui::Context,
}

impl Default for UserInterfaceContext {

    fn default() -> Self {
        Self::new()
    }

}

impl UserInterfaceContext {

    /// Creates a user interface context with the default professional theme
    pub fn new() -> Self {
        let egui_context: egui::Context = egui::Context::default();
        let default_theme: UserInterfaceTheme = UserInterfaceTheme::default();
        default_theme.apply(&egui_context);
        Self { egui_context }
    }

    /// Returns the underlying egui context for native windows, fonts, and customization
    pub fn egui_context(&self) -> &egui::Context {
        &self.egui_context
    }

    /// Runs one frame and returns its owned output without retaining a stale frame internally
    ///
    /// The compose closure is called once for each egui layout pass. egui normally performs one
    /// pass, but may call it again when a widget requests a discarded pass for multipass layout.
    /// Repaint requests made during composition are scheduled through egui's repaint callback when
    /// one is installed by the host; the root viewport's `repaint_delay` remains in the returned
    /// output for the host to schedule when no callback is available.
    pub fn run(
        &self,
        input: egui::RawInput,
        compose: impl FnMut(&mut UserInterface),
    ) -> egui::FullOutput {
        self.egui_context.run_ui(input, compose)
    }

    /// Applies a complete user interface theme while preserving egui interaction memory
    pub fn set_theme(&self, theme: &UserInterfaceTheme) {
        theme.apply(&self.egui_context);
    }

    /// Requests an immediate repaint of the root viewport
    pub fn request_repaint(&self) {
        self.egui_context.request_repaint();
    }

}
