// Copyright Rob Gage 2026

use crate::{
    UserInterface,
    Widget
};
use engine_graphics::Color;

/// Arranges `Widget`s from top to bottom
pub struct StackVertical {
    /// The child `Widget`s in this `StackVertical`
    children: Vec<Box<dyn Widget>>,
    /// The spacing between child `Widget`s in this `StackVertical`
    spacing: f32,
    /// The background color of this `StackVertical`
    background_color: Option<egui::Color32>,
    /// The fixed width of this stack within a horizontal stack, if any
    width: Option<f32>,
}

impl StackVertical {

    /// Creates an empty `StackVertical`
    pub const fn new() -> Self {
        Self { children: Vec::new(), spacing: 0.0, background_color: None, width: None }
    }

    /// Adds a widget to the end of the `StackVertical`
    pub fn with_child(mut self, child: impl Widget + 'static) -> Self {
        self.children.push(Box::new(child));
        self
    }

    /// Sets the space between children in this `StackVertical`
    pub const fn with_spacing(mut self, spacing: f32) -> Self {
        self.spacing = spacing;
        self
    }

    /// Sets the background color of this `StackVertical`
    pub fn with_background_color(mut self, color: &Color) -> Self {
        self.background_color = Some(color.into());
        self
    }

    /// Sets the stack width when it is contained in a horizontal stack
    pub const fn with_width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

}

impl Widget for StackVertical {

    fn display(&mut self, user_interface: &mut UserInterface) -> egui::Response {
        let available: egui::Rect = user_interface.available_rect();
        if let Some(background) = self.background_color {
            user_interface.painter().rect_filled(available, 0.0, background);
        }
        let spacing: f32 = self.spacing * self.children.len().saturating_sub(1) as f32;
        let fixed: f32 = self.children.iter().filter_map(|child| child.desired_height())
            .sum::<f32>();
        let flexible: usize =
            self.children.iter().filter(|child| child.desired_height().is_none()).count();
        let flexible_height: f32 =
            ((available.height() - spacing - fixed).max(0.0)) / flexible.max(1) as f32;
        let mut y: f32 = available.min.y;
        let mut response: egui::Response =
            user_interface.allocate_rect(available, egui::Sense::hover());
        for child in &mut self.children {
            let height: f32 = child.desired_height().map_or(flexible_height, |size| size);
            let rect: egui::Rect = egui::Rect::from_min_max(
                egui::pos2(available.min.x, y),
                egui::pos2(available.max.x, y + height),
            );
            response = user_interface.allocate_ui(rect, |ui| { child.display(ui); });
            y += height + self.spacing;
        }
        response
    }

    fn desired_width(&self) -> Option<f32> { self.width }

}
