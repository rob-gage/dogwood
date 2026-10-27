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
    MaterialPropertiesThermal,
};
use engine_graphics::world::materials::MaterialGraphicsCellularStatic;
use engine_host::acceleration::Accelerator;
use std::collections::{HashMap, HashSet};

/// Owns unresolved material definitions
pub struct MaterialRegistry {
    /// Names already registered, used to reject duplicates immediately
    names: HashSet<String>,
    /// Registered cellular static materials in registration order
    cellular_statics: Vec<Material>,
    /// Registered cellular dynamic materials in registration order
    cellular_dynamics: Vec<Material>,
    /// Registered fluid materials in registration order
    fluids: Vec<Material>,
    /// Registered gas materials in registration order
    gases: Vec<Material>,
}

impl MaterialRegistry {

    /// Creates an empty material registry
    pub fn new() -> Self {
        Self {
            names: HashSet::new(),
            cellular_statics: Vec::new(),
            cellular_dynamics: Vec::new(),
            fluids: Vec::new(),
            gases: Vec::new(),
        }
    }

    /// Registers an unresolved cellular static material by name
    pub fn register_cellular_static(
        &mut self,
        name: impl Into<String>,
        properties: MaterialPropertiesCellularStatic,
    ) -> Result<(), MaterialError> {
        let name: String = self.validate_name(name.into())?;
        self.cellular_statics.push(Material::CellularStatic { name, properties });
        Ok(())
    }

    /// Registers an unresolved cellular dynamic material by name
    pub fn register_cellular_dynamic(
        &mut self,
        name: impl Into<String>,
        properties: MaterialPropertiesCellularDynamic,
    ) -> Result<(), MaterialError> {
        let name: String = self.validate_name(name.into())?;
        self.cellular_dynamics.push(Material::CellularDynamic { name, properties });
        Ok(())
    }

    /// Registers an unresolved fluid material by name
    pub fn register_fluid(
        &mut self,
        name: impl Into<String>,
        properties: MaterialPropertiesFluid,
    ) -> Result<(), MaterialError> {
        let name: String = self.validate_name(name.into())?;
        self.fluids.push(Material::Fluid { name, properties });
        Ok(())
    }

    /// Registers an unresolved gas material by name
    pub fn register_gas(
        &mut self,
        name: impl Into<String>,
        properties: MaterialPropertiesGas,
    ) -> Result<(), MaterialError> {
        let name: String = self.validate_name(name.into())?;
        self.gases.push(Material::Gas { name, properties });
        Ok(())
    }

    /// Returns the definition addressed by this form-specific identifier position
    pub fn get(&self, identifier: MaterialIdentifier) -> Option<&Material> {
        if identifier == MaterialIdentifier::EMPTY { return None; }
        let index: usize = identifier.index() as usize;
        match identifier.form() {
            MaterialForm::CellularStatic => self.cellular_statics.get(index.checked_sub(1)?),
            MaterialForm::CellularDynamic => self.cellular_dynamics.get(index),
            MaterialForm::Fluid => self.fluids.get(index),
            MaterialForm::Gas => self.gases.get(index),
        }
    }

    /// Iterates over static edit colors, including the reserved empty slot
    pub fn cellular_static_edit_colors(
        &self,
    ) -> impl Iterator<
        Item = Option<&MaterialGraphicsCellularStatic>,
    > {
        std::iter::once(None).chain(self.cellular_statics.iter().map(|material| match material {
            Material::CellularStatic { properties, .. } => Some(&properties.edit_color),
            _ => None,
        }))
    }

    /// Returns the properties of a registered cellular static material
    pub fn cellular_static_properties(
        &self,
        identifier: MaterialIdentifier,
    ) -> Option<&MaterialPropertiesCellularStatic> {
        match self.get(identifier)? {
            Material::CellularStatic { properties, .. } => Some(properties),
            _ => None,
        }
    }

    /// Returns the properties of a registered cellular dynamic material
    pub fn cellular_dynamic_properties(
        &self,
        identifier: MaterialIdentifier,
    ) -> Option<&MaterialPropertiesCellularDynamic> {
        match self.get(identifier)? {
            Material::CellularDynamic { properties, .. } => Some(properties),
            _ => None,
        }
    }

    /// Returns the properties of a registered fluid material
    pub fn fluid_properties(
        &self,
        identifier: MaterialIdentifier,
    ) -> Option<&MaterialPropertiesFluid> {
        match self.get(identifier)? {
            Material::Fluid { properties, .. } => Some(properties),
            _ => None,
        }
    }

    /// Returns the properties of a registered gas material
    pub fn gas_properties(
        &self,
        identifier: MaterialIdentifier,
    ) -> Option<&MaterialPropertiesGas> {
        match self.get(identifier)? {
            Material::Gas { properties, .. } => Some(properties),
            _ => None,
        }
    }

    /// Compiles the registry, assigning identifiers and resolving all material names
    pub fn compile(self, accelerator: &Accelerator) -> Result<super::MaterialTable, MaterialError> {
        let mut identifiers_by_name: HashMap<String, MaterialIdentifier> = HashMap::new();
        for (form, materials, first_index) in [
            (MaterialForm::CellularStatic, &self.cellular_statics, 1usize),
            (MaterialForm::CellularDynamic, &self.cellular_dynamics, 0usize),
            (MaterialForm::Fluid, &self.fluids, 0usize),
            (MaterialForm::Gas, &self.gases, 0usize),
        ] {
            for (offset, material) in materials.iter().enumerate() {
                let index: u32 = first_index.checked_add(offset)
                    .ok_or(MaterialError::CapacityExceeded)?
                    .try_into().map_err(|_| MaterialError::CapacityExceeded)?;
                let identifier: MaterialIdentifier = MaterialIdentifier::new(form, index)?;
                identifiers_by_name.insert(material.name().to_owned(), identifier);
            }
        }
        let mut debris_by_identifier: HashMap<MaterialIdentifier, MaterialIdentifier> =
            HashMap::new();
        let mut transition_products: HashMap<(MaterialIdentifier, bool), MaterialIdentifier> =
            HashMap::new();
        for (form, materials, first_index) in [
            (MaterialForm::CellularStatic, &self.cellular_statics, 1usize),
            (MaterialForm::CellularDynamic, &self.cellular_dynamics, 0usize),
            (MaterialForm::Fluid, &self.fluids, 0usize),
            (MaterialForm::Gas, &self.gases, 0usize),
        ] {
            for (offset, material) in materials.iter().enumerate() {
                let index: u32 = (first_index + offset).try_into()
                    .map_err(|_| MaterialError::CapacityExceeded)?;
                let identifier: MaterialIdentifier = MaterialIdentifier::new(form, index)?;
                let source_name: &str = material.name();
                let thermal: Option<&MaterialPropertiesThermal> = match material {
                    Material::CellularStatic { properties, .. } => Some(&properties.thermal),
                    Material::CellularDynamic { properties, .. } => Some(&properties.thermal),
                    Material::Fluid { properties, .. } => Some(&properties.thermal),
                    Material::Gas { properties, .. } => Some(&properties.thermal),
                };
                if let Material::CellularStatic { properties, .. } = material {
                    if let Some(name) = properties.debris.as_deref() {
                        let Some(debris) = identifiers_by_name.get(name).copied() else {
                            return Err(MaterialError::UnknownMaterial {
                                name: name.to_owned(), referenced_by: source_name.to_owned(),
                            });
                        };
                        if debris.form() != MaterialForm::CellularDynamic {
                            return Err(MaterialError::InvalidDebris {
                                name: name.to_owned(), referenced_by: source_name.to_owned(),
                            });
                        }
                        debris_by_identifier.insert(identifier, debris);
                    }
                }
                if let Some(thermal) = thermal {
                    for (hot, transition) in [
                        (true, thermal.hot_transition.as_ref()),
                        (false, thermal.cold_transition.as_ref()),
                    ] {
                        let Some(transition) = transition else { continue; };
                        let Some(product) = identifiers_by_name.get(&transition.product).copied()
                        else {
                            return Err(MaterialError::UnknownMaterial {
                                name: transition.product.clone(),
                                referenced_by: source_name.to_owned(),
                            });
                        };
                        if product == identifier {
                            return Err(MaterialError::InvalidTransition {
                                name: source_name.to_owned(),
                                message: format!(
                                    "transition product '{}' must differ from its source",
                                    transition.product,
                                ),
                            });
                        }
                        transition_products.insert((identifier, hot), product);
                    }
                }
            }
        }
        super::MaterialTable::new(
            accelerator, self, identifiers_by_name, debris_by_identifier, transition_products,
        )
    }

    /// Returns the number of registered materials
    pub fn material_count(&self) -> usize {
        self.cellular_statics.len()
            + self.cellular_dynamics.len()
            + self.fluids.len()
            + self.gases.len()
    }

    /// Validates a name and records it so duplicate registrations are rejected
    fn validate_name(&mut self, name: String) -> Result<String, MaterialError> {
        if name.trim().is_empty() { return Err(MaterialError::EmptyName); }
        if !self.names.insert(name.clone()) {
            return Err(MaterialError::DuplicateName { name });
        }
        Ok(name)
    }

}
