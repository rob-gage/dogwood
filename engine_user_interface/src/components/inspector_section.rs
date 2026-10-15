// Copyright Rob Gage 2026

use egui::{
    CollapsingHeader,
    CollapsingResponse,
    Ui,
};

/// Collapsible titled section for inspector content
#[derive(Clone, Debug)]
pub struct InspectorSection {
    title: String,
    default_open: bool,
    id_salt: String,
}

impl InspectorSection {

    /// Create an inspector section that starts open
    pub fn new(title: impl Into<String>) -> Self {
        let title: String = title.into();
        Self {
            title: title.clone(),
            default_open: true,
            id_salt: title,
        }
    }

    /// Set whether this section starts expanded
    pub fn default_open(mut self, default_open: bool) -> Self {
        self.default_open = default_open;
        self
    }

    /// Set the stable egui identifier salt for this section
    pub fn id_salt(mut self, id_salt: impl Into<String>) -> Self {
        self.id_salt = id_salt.into();
        self
    }

    /// Show the inspector section and its contents
    pub fn show<R>(&self, ui: &mut Ui, add_contents: impl FnOnce(&mut Ui) -> R) -> CollapsingResponse<R> {
        CollapsingHeader::new(&self.title)
            .id_salt(&self.id_salt)
            .default_open(self.default_open)
            .show(ui, add_contents)
    }

}
