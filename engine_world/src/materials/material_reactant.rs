// Copyright Rob Gage 2026

/// A material or material tag consumed or catalyzed by a reaction
pub enum MaterialReactant {
    /// Selects one exact material name
    Material {
        /// Exact, case-sensitive material name
        name: String,
        /// Amount consumed per event, or zero for a catalyst
        amount: f32,
    },
    /// Selects every material carrying a tag
    Tag {
        /// Exact, case-sensitive tag
        tag: String,
        /// Amount consumed per event, or zero for a catalyst
        amount: f32,
    },
}

impl MaterialReactant {

    /// Creates a reactant selecting one material by name
    pub fn material(name: impl Into<String>, amount: f32) -> Self {
        Self::Material { name: name.into(), amount }
    }

    /// Creates a reactant selecting materials by tag
    pub fn tag(tag: impl Into<String>, amount: f32) -> Self {
        Self::Tag { tag: tag.into(), amount }
    }

}
