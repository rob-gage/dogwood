// Copyright Rob Gage 2026

use egui::{
    Frame,
    InnerResponse,
    Ui,
};

/// Compact horizontal command strip
#[derive(Clone, Copy, Debug, Default)]
pub struct Toolbar;

impl Toolbar {

    /// Show toolbar contents in a top-panel frame
    pub fn show<R>(ui: &mut Ui, add_contents: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
        let frame: Frame = Frame::side_top_panel(ui.style()).inner_margin(6.0);
        frame.show(ui, |ui: &mut Ui| ui.horizontal(add_contents).inner)
    }

}
