// Copyright Rob Gage 2026

use egui::{
    Frame,
    InnerResponse,
    Ui,
};

/// Compact bottom status strip
#[derive(Clone, Copy, Debug, Default)]
pub struct StatusBar;

impl StatusBar {

    /// Show status content in a bottom-panel frame
    pub fn show<R>(ui: &mut Ui, add_contents: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
        let frame: Frame = Frame::side_top_panel(ui.style()).inner_margin(6.0);
        frame.show(ui, |ui: &mut Ui| ui.horizontal(add_contents).inner)
    }

}
