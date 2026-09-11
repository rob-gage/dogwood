// Copyright Rob Gage 2026

use engine_user_interface::{
    UserInterface,
    Widget,
};
use std::{
    cell::Cell,
    rc::Rc,
};

/// Captures the editor's central content rectangle in physical surface pixels
pub struct ViewportArea(pub Rc<Cell<Option<[u32; 4]>>>);

impl Widget for ViewportArea {
    fn display(&mut self, user_interface: &mut UserInterface) -> egui::Response {
        let rect: egui::Rect = user_interface.available_rect();
        let scale: f32 = user_interface.pixels_per_point();
        self.0.set(Some([
            (rect.min.x * scale).round() as u32,
            (rect.min.y * scale).round() as u32,
            (rect.width() * scale).round() as u32,
            (rect.height() * scale).round() as u32,
        ]));
        user_interface.allocate_rect(rect, egui::Sense::hover())
    }
}
