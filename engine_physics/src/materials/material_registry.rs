// Copyright Rob Gage 2026

use super::{
    Material,
    MaterialForm,
    MaterialIdentifier
};
use std::ops::Index;

/// Registers `Material`s to `MaterialIdentifier`s
pub struct MaterialRegistry {
    /// The `Material::CellularStatic`s in this `MaterialRegistry`
    cellular_statics: Vec<Material>,
    /// The `Material::CellularDynamic`s in this `MaterialRegistry`
    cellular_dynamics: Vec<Material>,
    /// The `Material::Fluid`s in this `MaterialRegistry`
    fluids: Vec<Material>,
}

impl MaterialRegistry {

    /// Creates a new empty `MaterialRegistry`
    pub const fn new() -> Self {
        Self {
            cellular_statics: Vec::new(),
            cellular_dynamics: Vec::new(),
            fluids: Vec::new(),
        }
    }

    /// Registers a `Material` and returns its `MaterialIdentifier`
    pub fn register(&mut self, material: Material) -> MaterialIdentifier {
        match material {
            material @ Material::CellularStatic { .. } => {
                let index: u32 = self.cellular_statics.len() as u32;
                self.cellular_statics.push(material);
                MaterialIdentifier::new(MaterialForm::CellularStatic, index)
            }
            material @ Material::CellularDynamic { .. } => {
                let index: u32 = self.cellular_dynamics.len() as u32;
                self.cellular_dynamics.push(material);
                MaterialIdentifier::new(MaterialForm::CellularDynamic, index)
            }
            material @ Material::Fluid { .. } => {
                let index: u32 = self.fluids.len() as u32;
                self.fluids.push(material);
                MaterialIdentifier::new(MaterialForm::Fluid, index)
            }
        }
    }

}

impl Index<MaterialIdentifier> for MaterialRegistry {

    type Output = Material;

    fn index(&self, identifier: MaterialIdentifier) -> &Self::Output {
        let index: usize = identifier.index() as usize;
        match identifier.form() {
            MaterialForm::CellularStatic    => &self.cellular_statics[index],
            MaterialForm::CellularDynamic   => &self.cellular_dynamics[index],
            MaterialForm::Fluid             => &self.fluids[index],

        }
    }

}
