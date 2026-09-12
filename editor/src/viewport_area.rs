// Copyright Rob Gage 2026

use engine_user_interface::{
    UserInterface,
    Widget,
};
use engine_graphics::Color;
use std::{
    cell::Cell,
    rc::Rc,
};

/// Captures the editor's central content rectangle and draws its brush preview
pub struct ViewportArea {
    /// The editor content rectangle in physical surface pixels
    bounds: Rc<Cell<Option<[u32; 4]>>>,
    /// The clipped physical surface rectangles of preview cells
    preview_cells: Vec<[f32; 4]>,
    /// The fill color of preview cells
    preview_color: Color,
}

impl ViewportArea {

    /// Creates a `ViewportArea` with a cellular brush preview
    pub fn new(
        bounds: Rc<Cell<Option<[u32; 4]>>>,
        preview_cells: Vec<[f32; 4]>,
        preview_color: Color,
    ) -> Self { Self { bounds, preview_cells, preview_color } }

}

impl Widget for ViewportArea {
    fn display(&mut self, user_interface: &mut UserInterface) -> egui::Response {
        let rect: egui::Rect = user_interface.available_rect();
        let scale: f32 = user_interface.pixels_per_point();
        self.bounds.set(Some([
            (rect.min.x * scale).round() as u32,
            (rect.min.y * scale).round() as u32,
            (rect.width() * scale).round() as u32,
            (rect.height() * scale).round() as u32,
        ]));
        let painter: egui::Painter = user_interface.painter().with_clip_rect(rect);
        let preview_color: egui::Color32 = (&self.preview_color).into();
        for [left, top, right, bottom] in &self.preview_cells {
            let cell: egui::Rect = egui::Rect::from_min_max(
                egui::pos2(left / scale, top / scale),
                egui::pos2(right / scale, bottom / scale),
            );
            painter.rect_filled(cell, 0.0, preview_color);
            painter.rect_stroke(
                cell,
                0.0,
                egui::Stroke::new(1.0, egui::Color32::WHITE),
                egui::StrokeKind::Inside,
            );
        }
        user_interface.allocate_rect(rect, egui::Sense::hover())
    }
}
