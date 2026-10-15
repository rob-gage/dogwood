// Copyright Rob Gage 2026

use crate::egui::{Id, Ui};
use crate::egui_dock::{DockArea, DockState, Style, TabViewer};

/// Renders an app-owned dock state using the current egui theme
pub fn show_themed_dock_area<Tab>(
    ui: &mut Ui,
    dock_state: &mut DockState<Tab>,
    dock_area_id: Id,
    tab_viewer: &mut impl TabViewer<Tab = Tab>,
) {
    let dock_style: Style = Style::from_egui(ui.style());
    DockArea::new(dock_state)
        .id(dock_area_id)
        .style(dock_style)
        .show_inside(ui, tab_viewer);
}

