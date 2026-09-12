// Copyright Rob Gage 2026

use crate::{
    UserInterface,
    Widget,
};

/// A clickable text button
pub struct Button {
    /// The text displayed by this button
    text: String,
    /// Whether this button accepts interaction
    enabled: bool,
    /// The button width within a horizontal stack
    width: f32,
    /// The button height within a vertical stack
    height: f32,
    /// The action invoked when this button is clicked
    on_click: Box<dyn FnMut()>,
}

impl Button {

    /// Creates an enabled button with a click action
    pub fn new(text: impl Into<String>, on_click: impl FnMut() + 'static) -> Self {
        Self {
            text: text.into(),
            enabled: true,
            width: 96.0,
            height: 32.0,
            on_click: Box::new(on_click),
        }
    }

    /// Sets whether this button accepts interaction
    pub const fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

}

impl Widget for Button {

    fn display(&mut self, user_interface: &mut UserInterface) -> egui::Response {
        let response: egui::Response = user_interface.0.add_enabled(
            self.enabled,
            egui::Button::new(&self.text),
        );
        if response.clicked() { (self.on_click)(); }
        response
    }

    fn desired_width(&self) -> Option<f32> { Some(self.width) }

    fn desired_height(&self) -> Option<f32> { Some(self.height) }

}
