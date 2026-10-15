// Copyright Rob Gage 2026

use super::WorkspaceNavigationItem;
use egui::{
    collapsing_header::CollapsingState,
    InnerResponse,
    Response,
    Ui,
};

/// Shows one navigation entry with stable interaction and expansion state
pub(super) fn show_navigation_item(
    ui: &mut Ui,
    navigation_item: &WorkspaceNavigationItem,
    selected_identifier: &mut Option<String>,
    on_selected: &mut impl FnMut(&str),
) {
    let is_selected: bool = selected_identifier
        .as_deref()
        .is_some_and(|identifier: &str| identifier == navigation_item.identifier());
    let item_id: crate::egui::Id = ui.make_persistent_id(("workspace-navigation", navigation_item.identifier()));
    if navigation_item.children().is_empty() {
        let response: Response = ui.push_id(item_id, |ui: &mut Ui| {
            ui.selectable_label(is_selected, navigation_item.title())
        }).inner;
        if response.clicked() {
            *selected_identifier = Some(navigation_item.identifier().to_owned());
            on_selected(navigation_item.identifier());
        }
        return;
    }
    let collapsing_state: CollapsingState =
        CollapsingState::load_with_default_open(ui.ctx(), item_id, true);
    let _: (
        Response,
        InnerResponse<()>,
        Option<InnerResponse<()>>,
    ) = collapsing_state.show_header(ui, |ui: &mut Ui| {
        let response: Response = ui.push_id(item_id, |ui: &mut Ui| {
            ui.selectable_label(is_selected, navigation_item.title())
        }).inner;
        if response.clicked() {
            *selected_identifier = Some(navigation_item.identifier().to_owned());
            on_selected(navigation_item.identifier());
        }
    }).body(|ui: &mut Ui| {
        for child_item in navigation_item.children() {
            show_navigation_item(ui, child_item, selected_identifier, on_selected);
        }
    });
}

