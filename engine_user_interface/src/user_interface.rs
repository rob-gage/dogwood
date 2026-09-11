// Copyright Rob Gage 2026

use crate::Widget;

/// A wrapper around an `egui::Ui`.
pub struct UserInterface<'a>(pub(crate) &'a mut egui::Ui);

impl<'a> UserInterface<'a> {

    /// Adds a `Widget` and returns its response.
    pub fn add_widget(&mut self, widget: &mut impl Widget) -> egui::Response {
        widget.display(self)
    }

    /// Returns the painter used to draw this user interface.
    pub fn painter(&self) -> &egui::Painter { self.0.painter() }

    /// Returns the available area before the UI wraps its contents.
    pub fn available_rect(&self) -> egui::Rect { self.0.available_rect_before_wrap() }

    /// Returns the scale from interface points to physical surface pixels.
    pub fn pixels_per_point(&self) -> f32 { self.0.ctx().pixels_per_point() }

    /// Allocates an area for a widget.
    pub fn allocate_rect(&mut self, rect: egui::Rect, sense: egui::Sense) -> egui::Response {
        self.0.allocate_rect(rect, sense)
    }

    /// Displays contents inside a specific area of this user interface.
    pub fn allocate_ui(
        &mut self,
        rect: egui::Rect,
        add_contents: impl FnOnce(&mut UserInterface),
    ) -> egui::Response {
        self.0.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
            let mut ui = UserInterface(ui);
            add_contents(&mut ui);
        }).response
    }

    /// Creates an interactive region with a stable identifier.
    pub fn interact(
        &mut self,
        rect: egui::Rect,
        id: egui::Id,
        sense: egui::Sense,
    ) -> egui::Response {
        self.0.interact(rect, id, sense)
    }

}
