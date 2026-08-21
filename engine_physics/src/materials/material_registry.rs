// Copyright Rob Gage 2026

use super::{
    Material,
    MaterialAppearance,
    MaterialForm,
    MaterialIdentifier
};

/// Registers `Material`s to `MaterialIdentifier`s
pub struct MaterialRegistry {
    /// The names of `Material`s in this `MaterialRegistry`
    names: Vec<&'static str>,
    /// The `MaterialAppearance`s of `Material`s in this `MaterialRegistry`
    appearances: Vec<MaterialAppearance>,
    /// The `MaterialForm`s of `Material`s in this `MaterialRegistry`
    forms: Vec<MaterialForm>,
}

impl MaterialRegistry {

    /// Creates a new empty `MaterialRegistry`
    pub const fn new() -> Self {
        Self {
            names: Vec::new(),
            appearances: Vec::new(),
            forms: Vec::new(),
        }
    }

    /// Registers a `Material` and returns its `MaterialIdentifier`
    pub fn register(&mut self, material: Material) -> MaterialIdentifier {
        let identifier: MaterialIdentifier = MaterialIdentifier((self.names.len() + 1) as u32);
        self.names.push(material.name);
        self.appearances.push(material.appearance);
        self.forms.push(material.form);
        identifier
    }

}