// Copyright Rob Gage 2026

use egui::{Color32, CornerRadius, Painter, Rect, Shape, StrokeKind, Vec2};

use super::UiBrush;

/// The paintable visual state of one game UI element
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiVisual {
    /// The background brush
    pub background: UiBrush,
    /// The border stroke
    pub border: egui::Stroke,
    /// The background and border corner radius
    pub corner_radius: CornerRadius,
    /// A color multiplier applied to the complete visual
    pub tint: Color32,
    /// An opacity multiplier applied to this visual's background and border; text and
    /// control painting is handled separately
    pub opacity: f32,
    /// A paint offset in points
    pub offset: Vec2,
}

impl Default for UiVisual {

    /// Returns a neutral visual with no background or border
    fn default() -> Self {
        Self {
            background: UiBrush::None,
            border: egui::Stroke::NONE,
            corner_radius: CornerRadius::ZERO,
            tint: Color32::WHITE,
            opacity: 1.0,
            offset: Vec2::ZERO,
        }
    }
}

impl UiVisual {

    /// Paints the visual background and border
    pub fn paint(&self, painter: &Painter, rect: Rect) {
        painter.add(self.shape(rect));
    }

    pub(crate) fn shape(&self, rect: Rect) -> Shape {
        let painted_rect: Rect = rect.translate(self.offset);
        let opacity: f32 = if self.opacity.is_finite() {
            self.opacity.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let modulation: Color32 = self.tint.gamma_multiply(opacity);
        let mut shapes: Vec<Shape> = vec![self.background.shape_with_tint_and_corner_radius(painted_rect, modulation, self.corner_radius)];
        if !self.border.is_empty() {
            let border: egui::Stroke = egui::Stroke {
                width: self.border.width,
                color: self.border.color * modulation,
            };
            shapes.push(Shape::rect_stroke(painted_rect, self.corner_radius, border, StrokeKind::Inside));
        }
        Shape::Vec(shapes)
    }
}
