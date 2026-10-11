// Copyright Rob Gage 2026

use super::{
    Material,
    MaterialError,
    MaterialForm,
    MaterialIdentifier,
    MaterialPropertiesCellularDynamic,
    MaterialPropertiesCellularStatic,
    MaterialPropertiesFluid,
    MaterialPropertiesGas,
};
use engine_graphics::world::materials::MaterialGraphicsCellularStatic;

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
        let form: MaterialForm = material.form();
        let material_count: usize = match form {
            MaterialForm::CellularStatic => self.cellular_statics.len(),
            MaterialForm::CellularDynamic => self.cellular_dynamics.len(),
            MaterialForm::Fluid => self.fluids.len(),
            MaterialForm::Gas => self.gases.len(),
        };
        let material_index_offset: usize = match form {
            MaterialForm::CellularStatic => material_count.checked_add(1)
                .ok_or(MaterialError::CapacityExceeded)?,
            MaterialForm::CellularDynamic | MaterialForm::Fluid | MaterialForm::Gas => {
                material_count
            }
        };
        let material_index: u32 = material_index_offset
            .try_into()
            .map_err(|_| MaterialError::CapacityExceeded)?;
        let identifier: MaterialIdentifier = MaterialIdentifier::new(form, material_index)?;
        match material {
            material @ Material::CellularStatic { .. } => self.cellular_statics.push(material),
            material @ Material::CellularDynamic { .. } => self.cellular_dynamics.push(material),
            material @ Material::Fluid { .. } => self.fluids.push(material),
            material @ Material::Gas { .. } => self.gases.push(material),
        }
        Ok(identifier)
    }

    /// Iterates over the graphics of every cellular static material index, starting with empty
    pub fn cellular_static_graphics(
        &self,
    ) -> impl Iterator<Item = Option<&MaterialGraphicsCellularStatic>> {
        std::iter::once(None).chain(self.cellular_statics.iter().map(
            |material: &Material| match material {
                Material::CellularStatic { properties, .. } => Some(&properties.graphics),
                Material::CellularDynamic { .. }
                | Material::Fluid { .. }
                | Material::Gas { .. } => None,
            },
        ))
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

    /// Returns the properties of a registered cellular static material
    pub fn cellular_static_properties(
        &self,
        identifier: MaterialIdentifier,
    ) -> Option<&MaterialPropertiesCellularStatic> {
        match self.get(identifier)? {
            Material::CellularStatic { properties, .. } => Some(properties),
            Material::CellularDynamic { .. }
            | Material::Fluid { .. }
            | Material::Gas { .. } => None,
        }
    }

    /// Returns the properties of a registered cellular dynamic material
    pub fn cellular_dynamic_properties(
        &self,
        identifier: MaterialIdentifier,
    ) -> Option<&MaterialPropertiesCellularDynamic> {
        match self.get(identifier)? {
            Material::CellularDynamic { properties, .. } => Some(properties),
            Material::CellularStatic { .. }
            | Material::Fluid { .. }
            | Material::Gas { .. } => None,
        }
    }

    /// Returns the properties of a registered fluid material
    pub fn fluid_properties(
        &self,
        identifier: MaterialIdentifier,
    ) -> Option<&MaterialPropertiesFluid> {
        match self.get(identifier)? {
            Material::Fluid { properties, .. } => Some(properties),
            Material::CellularStatic { .. }
            | Material::CellularDynamic { .. }
            | Material::Gas { .. } => None,
        }
    }

    /// Returns the properties of a registered gas material
    pub fn gas_properties(
        &self,
        identifier: MaterialIdentifier,
    ) -> Option<&MaterialPropertiesGas> {
        match self.get(identifier)? {
            Material::Gas { properties, .. } => Some(properties),
            Material::CellularStatic { .. }
            | Material::CellularDynamic { .. }
            | Material::Fluid { .. } => None,
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
        if material.name().trim().is_empty() { return Err(MaterialError::EmptyName); }
        Ok(())
    }

}
