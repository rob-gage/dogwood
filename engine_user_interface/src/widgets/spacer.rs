// Copyright Rob Gage 2026

use crate::{
    UserInterface,
    Widget
};
use engine_graphics::Color;

/// An empty widget that reserves configurable space, transparent by default
pub struct Spacer {
    /// The size of this `Spacer` if it is not flexible
    size: Option<f32>,
    /// The background color of this `Spacer`
    background_color: Option<egui::Color32>,
}

impl Spacer {

    /// Creates a spacer with a fixed size
    pub const fn new(size: f32) -> Self {
        Self { size: Some(size), background_color: None }
    }

    /// Creates a spacer that expands to fill the space assigned by its stack
    pub const fn new_flexible() -> Self {
        Self { size: None, background_color: None }
    }

    /// Sets an optional background color.
    pub fn with_background_color(mut self, color: &Color) -> Self {
        self.background_color = Some(color.into());
        self
    }

}

impl Widget for Spacer {

    fn display(&mut self, user_interface: &mut UserInterface) -> egui::Response {
        let rect = user_interface.available_rect();
        if let Some(background) = self.background_color {
            user_interface.painter().rect_filled(rect, 0.0, background);
        }
        user_interface.allocate_rect(rect, egui::Sense::hover())
    }

    fn desired_width(&self) -> Option<f32> { self.size }

    fn desired_height(&self) -> Option<f32> { self.size }

}
