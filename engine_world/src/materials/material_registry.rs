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
    MaterialReaction,
    MaterialReactant,
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
    /// Registered reactions in priority order
    reactions: Vec<MaterialReaction>,
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
            reactions: Vec::new(),
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

    /// Registers a reaction after all previously registered reactions
    pub fn register_reaction(&mut self, reaction: MaterialReaction) {
        self.reactions.push(reaction);
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
    pub fn compile(
        mut self,
        accelerator: &Accelerator,
    ) -> Result<super::MaterialTable, MaterialError> {
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
        if self.reactions.len() > 64 {
            return Err(MaterialError::InvalidReaction {
                message: String::from("at most 64 reactions can be registered"),
            });
        }
        for reaction in &self.reactions {
            if !(1..=2).contains(&reaction.reactants.len())
                || reaction.products.len() > 3
                || !reaction.rate.is_finite() || reaction.rate <= 0.0
                || !valid_range(reaction.temperature_minimum, reaction.temperature_maximum)
                || !valid_range(reaction.pressure_minimum, reaction.pressure_maximum)
                || !reaction.heat.is_finite()
                || !reaction.pressure.is_finite() || reaction.pressure < 0.0
            {
                return Err(MaterialError::InvalidReaction {
                    message: String::from(
                        "reaction counts, rate, ranges, heat, or pressure are invalid",
                    ),
                });
            }
            let mut consumed: bool = false;
            for reactant in &reaction.reactants {
                let (amount, name): (f32, Option<&str>) = match reactant {
                    MaterialReactant::Material { name, amount } => (*amount, Some(name)),
                    MaterialReactant::Tag { amount, .. } => (*amount, None),
                };
                // a consumed amount below a gas amount unit would round to nothing
                if !amount.is_finite() || amount < 0.0 || (amount > 0.0 && amount < 1.0 / 4096.0) {
                    return Err(MaterialError::InvalidReaction {
                        message: String::from(
                            "reactant amounts must be zero or finite and at least 1/4096",
                        ),
                    });
                }
                consumed |= amount > 0.0;
                if let Some(name) = name && !identifiers_by_name.contains_key(name) {
                    return Err(MaterialError::UnknownReactantMaterial { name: name.to_owned() });
                }
                if let MaterialReactant::Tag { tag, .. } = reactant
                    && !self.materials_with_tag(tag).next().is_some()
                {
                    return Err(MaterialError::UnknownTag { tag: tag.clone() });
                }
            }
            if reaction.reactants.len() == 1 && match &reaction.reactants[0] {
                MaterialReactant::Material { amount, .. }
                | MaterialReactant::Tag { amount, .. } => *amount == 0.0,
            }
                || reaction.reactants.len() == 2 && !consumed
            {
                return Err(MaterialError::InvalidReaction {
                    message: String::from(
                        "a catalyst requires two reactants and at least one consumed reactant",
                    ),
                });
            }
            for product in &reaction.products {
                if !identifiers_by_name.contains_key(&product.product) {
                    return Err(MaterialError::UnknownProduct { name: product.product.clone() });
                }
                if !product.amount.is_finite() || product.amount < 1.0 / 4096.0
                    || !valid_reaction_transformation(&product.color)
                    || !valid_reaction_transformation(&product.radiance)
                {
                    return Err(MaterialError::InvalidReaction {
                        message: String::from(
                            "product amounts and color transformations are invalid",
                        ),
                    });
                }
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
        let reactions: Vec<MaterialReaction> = std::mem::take(&mut self.reactions);
        super::MaterialTable::new(
            accelerator, self, identifiers_by_name, debris_by_identifier, transition_products,
            reactions,
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

    /// Iterates over registered materials carrying a chemical tag
    fn materials_with_tag(&self, tag: &str) -> impl Iterator<Item = &Material> {
        self.cellular_statics.iter().chain(&self.cellular_dynamics)
            .chain(&self.fluids).chain(&self.gases).filter(move |material| {
                let chemical = match material {
                    Material::CellularStatic { properties, .. } => &properties.chemical,
                    Material::CellularDynamic { properties, .. } => &properties.chemical,
                    Material::Fluid { properties, .. } => &properties.chemical,
                    Material::Gas { properties, .. } => &properties.chemical,
                };
                chemical.tags.iter().any(|candidate| candidate == tag)
            })
    }

}

/// Returns whether a reaction range has valid nonnegative bounds
fn valid_range(minimum: f32, maximum: f32) -> bool {
    !minimum.is_nan() && !maximum.is_nan()
        && (minimum.is_finite() || minimum == f32::INFINITY) && minimum >= 0.0
        && (maximum.is_finite() || maximum == f32::INFINITY) && maximum >= 0.0
        && minimum <= maximum
}

/// Returns whether a reaction color transformation can be serialized
fn valid_reaction_transformation(
    transformation: &engine_graphics::world::materials::MaterialColorTransformation,
) -> bool {
    transformation.matrix.iter().flatten().chain(&transformation.offset)
        .all(|value| value.is_finite())
        && transformation.channel_variance.iter()
            .chain([&transformation.intensity_variance])
            .all(|value| value.is_finite() && *value >= 0.0)
}
