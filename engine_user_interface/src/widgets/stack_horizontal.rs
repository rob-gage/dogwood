// Copyright Rob Gage 2026

use crate::{
    UserInterface,
    Widget
};
use engine_graphics::Color;

/// Arranges `Widget`s from left to right
pub struct StackHorizontal {
    /// The child `Widget`s in this `StackHorizontal`
    children: Vec<Box<dyn Widget>>,
    /// The spacing between child `Widget`s in this `StackHorizontal`
    spacing: f32,
    /// The background color of this `StackHorizontal`
    background_color: Option<egui::Color32>,
}

impl StackHorizontal {

    /// Creates an empty `StackHorizontal`
    pub const fn new() -> Self {
        Self { children: Vec::new(), spacing: 0.0, background_color: None }
    }

    /// Adds a widget to the end of the `StackHorizontal`
    pub fn with_child(mut self, child: impl Widget + 'static) -> Self {
        self.children.push(Box::new(child));
        self
    }

    /// Sets the space between children in this `StackHorizontal`
    pub const fn with_spacing(mut self, spacing: f32) -> Self {
        self.spacing = spacing;
        self
    }

    /// Sets the background color of this `StackHorizontal`
    pub fn with_background_color(mut self, color: &Color) -> Self {
        self.background_color = Some(color.into());
        self
    }

}

impl Widget for StackHorizontal {

    fn display(&mut self, user_interface: &mut UserInterface) -> egui::Response {
        let available: egui::Rect = user_interface.available_rect();
        if let Some(background) = self.background_color {
            user_interface.painter().rect_filled(available, 0.0, background);
        }
        let spacing: f32 = self.spacing * self.children.len().saturating_sub(1) as f32;
        let fixed: f32 = self.children.iter().filter_map(|child| child.desired_size())
            .sum::<f32>();
        let flexible: usize =
            self.children.iter().filter(|child| child.desired_size().is_none()).count();
        let flexible_width: f32 =
            ((available.width() - spacing - fixed).max(0.0)) / flexible.max(1) as f32;
        let mut x: f32 = available.min.x;
        let mut response: egui::Response =
            user_interface.allocate_rect(available, egui::Sense::hover());
        for child in &mut self.children {
            let width: f32 = child.desired_size().map_or(flexible_width, |size| size);
            let rect: egui::Rect = egui::Rect::from_min_max(
                egui::pos2(x, available.min.y),
                egui::pos2(x + width, available.max.y),
            );
            response = user_interface.allocate_ui(rect, |ui| { child.display(ui); });
            x += width + self.spacing;
        }
        response
    }

}
