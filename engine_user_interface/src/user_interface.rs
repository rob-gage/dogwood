// Copyright Rob Gage 2026

use crate::Widget;

/// A wrapper around an `egui::Ui`.
pub struct UserInterface<'a>(pub(crate) &'a mut egui::Ui);

impl<'a> UserInterface<'a> {

    /// Adds a `Widget` and returns its response.
    pub fn add_widget(&mut self, widget: impl Widget) -> egui::Response { widget.display(self) }

}
