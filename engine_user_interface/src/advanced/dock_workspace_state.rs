// Copyright Rob Gage 2026

use crate::egui_dock::{DockState, TabPath};

/// App-owned docking state for an editor workspace
pub struct DockWorkspaceState<Tab> {
    dock_state: DockState<Tab>,
}

impl<Tab> DockWorkspaceState<Tab> {

    /// Creates a workspace with the supplied tabs in the main surface
    pub fn new(tabs: Vec<Tab>) -> Self {
        Self {
            dock_state: DockState::new(tabs),
        }
    }

    /// Borrows the underlying egui_dock state
    pub fn dock_state(&self) -> &DockState<Tab> {
        &self.dock_state
    }

    /// Borrows the underlying egui_dock state mutably
    pub fn dock_state_mut(&mut self) -> &mut DockState<Tab> {
        &mut self.dock_state
    }

    /// Selects a tab at an egui_dock path
    pub fn set_active_tab(&mut self, tab_path: TabPath) -> crate::egui_dock::Result {
        self.dock_state.set_active_tab(tab_path)
    }

}

