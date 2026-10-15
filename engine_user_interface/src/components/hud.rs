// Copyright Rob Gage 2026

use std::fmt::Debug;
use std::hash::Hash;
use egui::{
    Align2,
    Area,
    Frame,
    Id,
    InnerResponse,
    Order,
    Ui,
};

/// Foreground heads-up display region
#[derive(Clone, Debug)]
pub struct Hud {
    identifier: Id,
    anchor: Align2,
    offset: egui::Vec2,
}

impl Hud {

    /// Create a HUD anchored to the top-left corner
    pub fn new(identifier: impl Hash + Debug) -> Self {
        Self {
            identifier: Id::new(identifier),
            anchor: Align2::LEFT_TOP,
            offset: egui::Vec2::ZERO,
        }
    }

    /// Set the HUD anchor and offset in interface points
    pub fn anchor(mut self, anchor: Align2, offset: egui::Vec2) -> Self {
        self.anchor = anchor;
        self.offset = offset;
        self
    }

    /// Show HUD content above ordinary windows
    pub fn show<R>(&self, context: &egui::Context, add_contents: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
        Area::new(self.identifier)
            .anchor(self.anchor, self.offset)
            .order(Order::Foreground)
            .show(context, |ui: &mut Ui| Frame::popup(ui.style()).show(ui, add_contents).inner)
    }

}
