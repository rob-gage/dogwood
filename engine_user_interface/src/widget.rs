// Copyright Rob Gage 2026

use crate::UserInterface;

/// A user-interface widget that can be added to a `UserInterface`
pub trait Widget {

    /// Displays this widget and returns its response
    fn display(self, user_interface: &mut UserInterface) -> egui::Response;

}
