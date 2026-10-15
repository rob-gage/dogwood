// Copyright Rob Gage 2026

//! Computed absolute game UI geometry

/// Absolute border-box and content-box geometry for one layout node
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiComputedLayout {
    /// Absolute border-box rectangle
    pub rect: egui::Rect,
    /// Absolute allocated content-box rectangle
    pub content_rect: egui::Rect,
    /// Content size in logical points, including Taffy-reported overflow
    pub content_size: egui::Vec2,
}

impl Default for UiComputedLayout {

    fn default() -> Self {
        Self { rect: egui::Rect::ZERO, content_rect: egui::Rect::ZERO, content_size: egui::Vec2::ZERO }
    }
}
