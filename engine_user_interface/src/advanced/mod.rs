// Copyright Rob Gage 2026

//! Editor-oriented docking and navigation helpers

mod show_themed_dock_area;
mod dock_workspace_state;
mod show_navigation_item;
mod show_workspace_navigation;
mod workspace_navigation_item;

pub use show_themed_dock_area::show_themed_dock_area;
pub use dock_workspace_state::DockWorkspaceState;
pub use show_workspace_navigation::show_workspace_navigation;
pub use workspace_navigation_item::WorkspaceNavigationItem;

