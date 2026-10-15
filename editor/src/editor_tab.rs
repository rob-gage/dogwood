// Copyright Rob Gage 2026

/// A docked tab of the editor workspace
#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq)]
pub enum EditorTab {
    /// Hosted game drawn at the size of the tab
    Viewport,
    /// Materials, brush, and view settings
    Tools,
}
