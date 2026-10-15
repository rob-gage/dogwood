// Copyright Rob Gage 2026

use super::{
    WorkspaceNavigationItem,
    show_navigation_item::show_navigation_item,
};
use egui::Ui;

/// Shows a stable-ID navigation tree and reports the selected identifier
///
/// The selection callback runs during composition and may run on multiple layout passes
/// Record an idempotent pending action and perform external effects after the frame returns
pub fn show_workspace_navigation(
    ui: &mut Ui,
    navigation_items: &[WorkspaceNavigationItem],
    selected_identifier: &mut Option<String>,
    mut on_selected: impl FnMut(&str),
) {
    for navigation_item in navigation_items {
        show_navigation_item(ui, navigation_item, selected_identifier, &mut on_selected);
    }
}

