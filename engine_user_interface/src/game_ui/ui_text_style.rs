// Copyright Rob Gage 2026

use std::sync::Arc;

use egui::{
    Align,
    Color32,
    FontId,
    Galley,
    Ui,
    text::{
        LayoutJob,
        TextFormat,
        TextWrapping,
    },
};

/// Typography settings for a game UI text element
#[derive(Clone, Debug, PartialEq)]
pub struct UiTextStyle {
    pub font: FontId,
    pub color: Color32,
    pub align: Align,
    pub wrap: bool,
    pub line_height: Option<f32>,
}

impl UiTextStyle {

    /// Builds the final egui galley for the requested width
    pub fn layout(&self, ui: &Ui, text: &str, width: f32) -> Arc<Galley> {
        let text_format: TextFormat = TextFormat {
            font_id: self.font.clone(),
            color: self.color,
            line_height: self.line_height,
            ..Default::default()
        };
        let mut layout_job: LayoutJob = LayoutJob::single_section(text.to_owned(), text_format);
        let wrapping_width: f32 = if width.is_finite() {
            width.max(0.0)
        } else {
            f32::INFINITY
        };
        layout_job.halign = self.align;
        layout_job.wrap = if self.wrap {
            TextWrapping::wrap_at_width(wrapping_width)
        } else {
            TextWrapping::no_max_width()
        };
        ui.painter().layout_job(layout_job)
    }

}

impl Default for UiTextStyle {

    fn default() -> Self {
        Self {
            font: FontId::proportional(14.0),
            color: Color32::WHITE,
            align: Align::LEFT,
            wrap: true,
            line_height: None,
        }
    }

}
