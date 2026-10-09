// Copyright Rob Gage 2026

use super::{
    Material,
    MaterialError,
    MaterialForm,
    MaterialIdentifier
};

/// Owns registered materials and their identifiers
pub struct MaterialRegistry {
    /// The `Material::CellularStatic`s in this `MaterialRegistry`
    cellular_statics: Vec<Material>,
    /// The `Material::CellularDynamic`s in this `MaterialRegistry`
    cellular_dynamics: Vec<Material>,
    /// The `Material::Fluid`s in this `MaterialRegistry`
    fluids: Vec<Material>,
    /// The `Material::Gas`s in this `MaterialRegistry`
    gases: Vec<Material>,
}

impl MaterialRegistry {

    /// Creates an empty material registry
    pub const fn new() -> Self {
        Self {
            cellular_statics: Vec::new(),
            cellular_dynamics: Vec::new(),
            fluids: Vec::new(),
            gases: Vec::new(),
        }
    }

    /// Registers a material and returns its stable form-specific identifier
    pub fn register(&mut self, material: Material) -> Result<MaterialIdentifier, MaterialError> {
        Self::validate_material(&material)?;
        let form: MaterialForm = material.form;
        let materials: &mut Vec<Material> = match form {
            MaterialForm::CellularStatic => &mut self.cellular_statics,
            MaterialForm::CellularDynamic => &mut self.cellular_dynamics,
            MaterialForm::Fluid => &mut self.fluids,
            MaterialForm::Gas => &mut self.gases,
        };
        let material_index_offset: usize = match form {
            MaterialForm::CellularStatic => materials.len().checked_add(1)
                .ok_or(MaterialError::CapacityExceeded)?,
            MaterialForm::CellularDynamic | MaterialForm::Fluid | MaterialForm::Gas => {
                materials.len()
            }
        };
        let material_index: u32 = material_index_offset
            .try_into()
            .map_err(|_| MaterialError::CapacityExceeded)?;
        let identifier: MaterialIdentifier = MaterialIdentifier::new(form, material_index)?;
        materials.push(material);
        Ok(identifier)
    }

    /// Returns the material represented by a valid registered identifier
    pub fn get(&self, identifier: MaterialIdentifier) -> Option<&Material> {
        if identifier == MaterialIdentifier::EMPTY { return None; }
        let material_form_index: usize = identifier.index() as usize;
        match identifier.form() {
            MaterialForm::CellularStatic => self
                .cellular_statics
                .get(material_form_index.checked_sub(1)?),
            MaterialForm::CellularDynamic => self.cellular_dynamics.get(material_form_index),
            MaterialForm::Fluid => self.fluids.get(material_form_index),
            MaterialForm::Gas => self.gases.get(material_form_index),
        }
    }

    /// Returns the number of registered materials
    pub fn material_count(&self) -> usize {
        self.cellular_statics.len()
            + self.cellular_dynamics.len()
            + self.fluids.len()
            + self.gases.len()
    }

    fn validate_material(material: &Material) -> Result<(), MaterialError> {
        if material.name.trim().is_empty() { return Err(MaterialError::EmptyName); }
        Ok(())
    }

}
