// Copyright Rob Gage 2026

/// A stable editor navigation entry with optional nested entries
pub struct WorkspaceNavigationItem {
    identifier: String,
    title: String,
    children: Vec<Self>,
}

impl WorkspaceNavigationItem {

    /// Creates a leaf navigation entry
    pub fn new(identifier: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            identifier: identifier.into(),
            title: title.into(),
            children: Vec::new(),
        }
    }

    /// Adds a nested entry and returns the updated parent
    pub fn with_children(mut self, children: impl IntoIterator<Item = Self>) -> Self {
        self.children = children.into_iter().collect();
        self
    }

    /// Returns the stable identifier used for egui state and selection
    pub fn identifier(&self) -> &str {
        &self.identifier
    }

    /// Returns the visible navigation title
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Returns nested navigation entries
    pub fn children(&self) -> &[Self] {
        &self.children
    }

}

