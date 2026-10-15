// Copyright Rob Gage 2026

use egui::{Color32, Painter, Rect, Shape};

use super::{UiImage, UiNineSlice};

/// A solid, image, or nine-slice visual fill
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum UiBrush {
    /// Paint nothing
    None,
    /// Paint one solid color
    Color(Color32),
    /// Paint a fitted image
    Image(UiImage),
    /// Paint a nine-slice image
    NineSlice(UiNineSlice),
}

impl Default for UiBrush {

    /// Returns a brush that paints nothing
    fn default() -> Self {
        Self::None
    }
}

impl UiBrush {

    /// Paints this brush without additional visual modulation
    pub fn paint(&self, painter: &Painter, rect: Rect) {
        self.paint_with_tint_and_corner_radius(
            painter,
            rect,
            Color32::WHITE,
            egui::CornerRadius::ZERO,
        )
    }

    /// Paints this brush with visual modulation and a background corner radius
    pub(crate) fn paint_with_tint_and_corner_radius(
        &self,
        painter: &Painter,
        rect: Rect,
        tint: Color32,
        corner_radius: egui::CornerRadius,
    ) {
        painter.add(self.shape_with_tint_and_corner_radius(rect, tint, corner_radius));
    }

    pub(crate) fn shape_with_tint_and_corner_radius(&self, rect: Rect, tint: Color32, corner_radius: egui::CornerRadius) -> Shape {
        match self {
            Self::None => Shape::Noop,
            Self::Color(color) => Shape::rect_filled(rect, corner_radius, *color * tint),
            Self::Image(image) if tint == Color32::WHITE => image.shape(rect),
            Self::Image(image) => image.shape_with_tint(rect, image.tint * tint),
            Self::NineSlice(nine_slice) if tint == Color32::WHITE => nine_slice.shape(rect),
            Self::NineSlice(nine_slice) => nine_slice.shape_with_tint(rect, nine_slice.image.tint * tint),
        }
    }
}
