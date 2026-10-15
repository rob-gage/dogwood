// Copyright Rob Gage 2026

//! Flattened game UI layout nodes

use super::UiLayout;

/// A flattened pre-order layout node
#[derive(Clone, Debug, PartialEq)]
pub struct UiLayoutNode {
    /// Stable element identity used by the reusable Taffy cache
    pub id: egui::Id,
    /// Taffy-backed layout properties
    pub layout: UiLayout,
    /// Child indexes in the flattened node array
    pub children: Vec<usize>,
}

impl UiLayoutNode {

    /// Creates a node with no children
    #[must_use]
    pub fn new(id: egui::Id, layout: UiLayout) -> Self {
        Self { id, layout, children: Vec::new() }
    }

    /// Creates a node with the supplied flattened child indexes
    #[must_use]
    pub fn with_children(id: egui::Id, layout: UiLayout, children: Vec<usize>) -> Self {
        Self { id, layout, children }
    }
}
