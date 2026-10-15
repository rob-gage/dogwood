// Copyright Rob Gage 2026

use egui::{
    Frame,
    InnerResponse,
    Ui,
};

/// Framed content region using the active egui panel visuals
#[derive(Clone, Copy, Debug, Default)]
pub struct StyledPanel;

impl StyledPanel {

    /// Show content in a softly bordered panel
    pub fn show<R>(ui: &mut Ui, add_contents: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
        let frame: Frame = Frame::group(ui.style()).inner_margin(10.0);
        frame.show(ui, add_contents)
    }

}
