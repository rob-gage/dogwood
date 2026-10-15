// Copyright Rob Gage 2026

use egui::{
    InnerResponse,
    Ui,
};

/// Label-and-control row for settings forms
#[derive(Clone, Copy, Debug, Default)]
pub struct SettingsRow;

impl SettingsRow {

    /// Show a label on the left and controls in the remaining space
    pub fn show<R>(ui: &mut Ui, label: impl Into<egui::WidgetText>, add_control: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
        ui.horizontal(|ui: &mut Ui| {
            ui.label(label);
            ui.add_space(12.0);
            add_control(ui)
        })
    }

}
