// Copyright Rob Gage 2026

use crate::UserInterface;

/// A user-interface widget that can be added to a `UserInterface`
pub trait Widget {

    /// Displays this widget and returns its `egui::Response`
    fn display(&mut self, user_interface: &mut UserInterface) -> egui::Response;

    /// Returns the widget's fixed size along its containing stack's axis, if it has one
    fn desired_size(&self) -> Option<f32> { None }

}
