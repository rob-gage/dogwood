// Copyright Rob Gage 2026

use super::{
    CompiledMaterialReaction, CompiledMaterialReactionProduct, CompiledMaterialReactionReactant,
    Material, MaterialForm, MaterialIdentifier, MaterialReaction, MaterialReference,
    MaterialThermalProperties,
};
use engine_compute::Accelerator;
use engine_graphics::MaterialGraphics;
use std::{collections::BTreeMap, ops::Index};

/// Registers `Material`s to `MaterialIdentifier`s
pub struct MaterialRegistry {
    /// The `Material::CellularStatic`s in this `MaterialRegistry`
    pub(super) cellular_statics: Vec<Material>,
    /// The `Material::CellularDynamic`s in this `MaterialRegistry`
    pub(super) cellular_dynamics: Vec<Material>,
    /// The `Material::Fluid`s in this `MaterialRegistry`
    pub(super) fluids: Vec<Material>,
    /// The `Material::Gas`s in this `MaterialRegistry`
    pub(super) gases: Vec<Material>,
    pub(super) thermal: Vec<MaterialThermalProperties>,
    pub(super) tags: BTreeMap<String, Vec<MaterialIdentifier>>,
    pub(super) reactions: Vec<CompiledMaterialReaction>,
    pub(super) reaction_selector_members: Vec<MaterialIdentifier>,
}

impl Default for MaterialRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl MaterialRegistry {
    /// Creates a new empty `MaterialRegistry`
    pub const fn new() -> Self {
        Self {
            cellular_statics: Vec::new(),
            cellular_dynamics: Vec::new(),
            fluids: Vec::new(),
            gases: Vec::new(),
            thermal: Vec::new(),
            tags: BTreeMap::new(),
            reactions: Vec::new(),
            reaction_selector_members: Vec::new(),
        }
    }

    /// Registers a `Material` and returns its `MaterialIdentifier`
    pub fn register(&mut self, material: Material) -> MaterialIdentifier {
        assert!(Self::material_is_valid(&material));
        let identifier = match material {
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
            material @ Material::Gas { .. } => {
                let index: u32 = self
                    .gases
                    .len()
                    .try_into()
                    .expect("Too many registered gas materials");
                self.gases.push(material);
                MaterialIdentifier::new(MaterialForm::Gas, index)
            }
        };
        self.thermal = vec![MaterialThermalProperties::default(); self.material_count() as usize];
        identifier
    }

    /// Number of registered materials in stable form-major order.
    pub fn material_count(&self) -> u32 {
        (self.cellular_statics.len()
            + self.cellular_dynamics.len()
            + self.fluids.len()
            + self.gases.len()) as u32
    }
    /// Stable form-major index used by compiled reaction metadata.
    pub fn dense_index(&self, identifier: MaterialIdentifier) -> Option<u32> {
        self.get(identifier)?;
        let offset = match identifier.form_checked()? {
            MaterialForm::CellularStatic => 0,
            MaterialForm::CellularDynamic => self.cellular_statics.len(),
            MaterialForm::Fluid => self.cellular_statics.len() + self.cellular_dynamics.len(),
            MaterialForm::Gas => {
                self.cellular_statics.len() + self.cellular_dynamics.len() + self.fluids.len()
            }
        };
        Some((offset + identifier.index() as usize) as u32)
    }
    pub fn identifier_from_dense_index(&self, index: u32) -> Option<MaterialIdentifier> {
        let index = index as usize;
        let s = self.cellular_statics.len();
        let d = s + self.cellular_dynamics.len();
        let f = d + self.fluids.len();
        if index < s {
            Some(MaterialIdentifier::new(
                MaterialForm::CellularStatic,
                index as u32,
            ))
        } else if index < d {
            Some(MaterialIdentifier::new(
                MaterialForm::CellularDynamic,
                (index - s) as u32,
            ))
        } else if index < f {
            Some(MaterialIdentifier::new(
                MaterialForm::Fluid,
                (index - d) as u32,
            ))
        } else if index < self.material_count() as usize {
            Some(MaterialIdentifier::new(
                MaterialForm::Gas,
                (index - f) as u32,
            ))
        } else {
            None
        }
    }
    pub fn thermal_properties(
        &self,
        identifier: MaterialIdentifier,
    ) -> Option<&MaterialThermalProperties> {
        self.dense_index(identifier)
            .and_then(|index| self.thermal.get(index as usize))
    }
    pub fn tag_members(&self, tag: &str) -> Option<&[MaterialIdentifier]> {
        self.tags.get(tag).map(Vec::as_slice)
    }
    /// Fixed-stride, Accelerator-ready reaction metadata in stable authoring order.
    pub fn reactions(&self) -> &[CompiledMaterialReaction] {
        &self.reactions
    }
    /// Material IDs used by compiled reaction selector ranges.
    pub fn reaction_selector_members(&self) -> &[MaterialIdentifier] {
        &self.reaction_selector_members
    }
    pub(crate) fn set_compiled_metadata(
        &mut self,
        metadata: BTreeMap<MaterialIdentifier, MaterialThermalProperties>,
        mut tags: BTreeMap<String, Vec<MaterialIdentifier>>,
        reactions: Vec<MaterialReaction>,
    ) -> Result<(), String> {
        let mut thermal =
            vec![MaterialThermalProperties::default(); self.material_count() as usize];
        for (identifier, properties) in metadata {
            let index = self
                .dense_index(identifier)
                .ok_or("Thermal metadata references an unregistered material")?
                as usize;
            Self::validate_thermal(identifier, &properties, self)?;
            thermal[index] = properties;
        }
        for members in tags.values_mut() {
            members.sort_unstable();
            members.dedup();
            if members.iter().any(|id| self.get(*id).is_none()) {
                return Err("Tag references an unregistered material".into());
            }
        }
        self.thermal = thermal;
        let (compiled_reactions, selector_members) =
            Self::compile_reactions(&reactions, &tags, self)?;
        self.tags = tags;
        self.reactions = compiled_reactions;
        self.reaction_selector_members = selector_members;
        Ok(())
    }
    fn compile_reactions(
        reactions: &[MaterialReaction],
        tags: &BTreeMap<String, Vec<MaterialIdentifier>>,
        registry: &Self,
    ) -> Result<(Vec<CompiledMaterialReaction>, Vec<MaterialIdentifier>), String> {
        let mut compiled = Vec::with_capacity(reactions.len());
        let mut members = Vec::new();
        for (order, reaction) in reactions.iter().enumerate() {
            let finite = |value: f32| value.is_finite();
            if !finite(reaction.maximum_extent_per_tick)
                || reaction.maximum_extent_per_tick < 0.0
                || !finite(reaction.thermal_energy)
                || !finite(reaction.pressure_output)
                || reaction
                    .minimum_temperature
                    .is_some_and(|v| !finite(v) || v < 0.0)
                || reaction
                    .maximum_temperature
                    .is_some_and(|v| !finite(v) || v < 0.0)
                || reaction
                    .minimum_pressure
                    .is_some_and(|v| !finite(v) || v < 0.0)
                || reaction
                    .maximum_pressure
                    .is_some_and(|v| !finite(v) || v < 0.0)
                || reaction
                    .minimum_air
                    .is_some_and(|v| !finite(v) || !(0.0..=1.0).contains(&v))
                || reaction
                    .maximum_air
                    .is_some_and(|v| !finite(v) || !(0.0..=1.0).contains(&v))
            {
                return Err("Invalid reaction numeric value".into());
            }
            if reaction
                .minimum_temperature
                .zip(reaction.maximum_temperature)
                .is_some_and(|(min, max)| min > max)
                || reaction
                    .minimum_pressure
                    .zip(reaction.maximum_pressure)
                    .is_some_and(|(min, max)| min > max)
                || reaction
                    .minimum_air
                    .zip(reaction.maximum_air)
                    .is_some_and(|(min, max)| min > max)
            {
                return Err("Invalid reaction range".into());
            }
            let has_environment = reaction.minimum_temperature.is_some()
                || reaction.maximum_temperature.is_some()
                || reaction.minimum_pressure.is_some()
                || reaction.maximum_pressure.is_some()
                || reaction.minimum_air.is_some()
                || reaction.maximum_air.is_some();
            let reactant_count = reaction.reactants.iter().flatten().count();
            if reactant_count == 0 && !has_environment {
                return Err("Unconditional zero-reactant reaction is invalid".into());
            }
            if reaction.products.iter().flatten().count() == 0
                && reaction.thermal_energy == 0.0
                && reaction.pressure_output == 0.0
            {
                return Err("Reaction has no observable output".into());
            }
            let mut compiled_reactants = [CompiledMaterialReactionReactant::default(); 2];
            for (i, reactant) in reaction.reactants.iter().enumerate() {
                let Some(reactant) = reactant else { continue };
                if !finite(reactant.amount) || reactant.amount <= 0.0 {
                    return Err("Invalid reaction reactant amount".into());
                }
                let resolved: Vec<MaterialIdentifier> = match &reactant.selector {
                    MaterialReference::Material(id) => {
                        if registry.get(*id).is_none() {
                            return Err("Reaction references an unregistered material".into());
                        }
                        vec![*id]
                    }
                    MaterialReference::Tag(tag) => tags
                        .get(tag)
                        .cloned()
                        .ok_or("Reaction references an unknown tag")?,
                };
                if resolved.is_empty() {
                    return Err("Reaction selector cannot match an empty tag".into());
                }
                let offset: u32 = members
                    .len()
                    .try_into()
                    .map_err(|_| "Too many reaction selector members")?;
                let count: u32 = resolved
                    .len()
                    .try_into()
                    .map_err(|_| "Too many reaction selector members")?;
                members.extend(resolved);
                compiled_reactants[i] = CompiledMaterialReactionReactant {
                    member_offset: offset,
                    member_count: count,
                    amount_bits: reactant.amount.to_bits(),
                    present: 1,
                };
            }
            let mut compiled_products = [CompiledMaterialReactionProduct::default(); 2];
            for (i, product) in reaction.products.iter().enumerate() {
                let Some(product) = product else { continue };
                if registry.get(product.material).is_none() {
                    return Err("Reaction product references an unregistered material".into());
                }
                if !finite(product.amount) || product.amount <= 0.0 {
                    return Err("Invalid reaction product amount".into());
                }
                compiled_products[i] = CompiledMaterialReactionProduct {
                    material: product.material.as_u32(),
                    amount_bits: product.amount.to_bits(),
                    present: 1,
                    _padding: 0,
                };
            }
            compiled.push(CompiledMaterialReaction {
                reactants: compiled_reactants,
                products: compiled_products,
                minimum_temperature: reaction.minimum_temperature.unwrap_or(f32::NAN),
                maximum_temperature: reaction.maximum_temperature.unwrap_or(f32::NAN),
                minimum_pressure: reaction.minimum_pressure.unwrap_or(f32::NAN),
                maximum_pressure: reaction.maximum_pressure.unwrap_or(f32::NAN),
                minimum_air: reaction.minimum_air.unwrap_or(f32::NAN),
                maximum_extent_per_tick: reaction.maximum_extent_per_tick,
                maximum_air: reaction.maximum_air.unwrap_or(f32::NAN),
                thermal_energy: reaction.thermal_energy,
                pressure_output: reaction.pressure_output,
                priority: reaction.priority,
                authoring_order: order as u32,
            });
        }
        Ok((compiled, members))
    }
    fn validate_thermal(
        identifier: MaterialIdentifier,
        properties: &MaterialThermalProperties,
        registry: &Self,
    ) -> Result<(), String> {
        if !properties.conductivity.is_finite()
            || properties.conductivity < 0.0
            || !properties.specific_heat_capacity.is_finite()
            || properties.specific_heat_capacity <= 0.0
            || properties
                .default_temperature
                .is_some_and(|value| !value.is_finite() || value < 0.0)
        {
            return Err("Invalid thermal properties".into());
        }
        for transition in [&properties.cold_transition, &properties.hot_transition]
            .into_iter()
            .flatten()
        {
            if !transition.threshold_temperature.is_finite()
                || transition.threshold_temperature < 0.0
                || !transition.latent_energy.is_finite()
                || transition.latent_energy < 0.0
                || !transition.yield_rate.is_finite()
                || !(0.0..=1.0).contains(&transition.yield_rate)
                || transition.target == identifier
                || registry.get(transition.target).is_none()
            {
                return Err("Invalid thermal transition".into());
            }
        }
        if let (Some(cold), Some(hot)) = (&properties.cold_transition, &properties.hot_transition)
            && cold.threshold_temperature >= hot.threshold_temperature
        {
            return Err("Cold transition must precede hot transition".into());
        }
        Ok(())
    }

    /// Returns whether all simulation properties of a material are valid
    pub(super) fn material_is_valid(material: &Material) -> bool {
        match material {
            Material::CellularStatic {
                mass,
                minimum_rigid_body_cell_count,
                pressure_transmission,
                friction,
                restitution,
                ..
            } => {
                mass.is_finite()
                    && *mass > 0.0
                    && *minimum_rigid_body_cell_count > 0
                    && pressure_transmission.is_finite()
                    && (0.0..=1.0).contains(pressure_transmission)
                    && friction.is_finite()
                    && (0.0..=1.0).contains(friction)
                    && restitution.is_finite()
                    && (0.0..=1.0).contains(restitution)
            }
            Material::CellularDynamic {
                mass,
                pressure_transmission,
                friction,
                restitution,
                ..
            } => {
                mass.is_finite()
                    && *mass > 0.0
                    && pressure_transmission.is_finite()
                    && (0.0..=1.0).contains(pressure_transmission)
                    && friction.is_finite()
                    && (0.0..=1.0).contains(friction)
                    && restitution.is_finite()
                    && (0.0..=1.0).contains(restitution)
            }
            Material::Fluid {
                friction,
                restitution,
                rest_density,
                artificial_pressure,
                xsph_smoothing,
                body_push_speed,
                density,
                viscosity,
                ..
            } => {
                friction.is_finite()
                    && (0.0..=1.0).contains(friction)
                    && restitution.is_finite()
                    && (0.0..=1.0).contains(restitution)
                    && rest_density.is_finite()
                    && *rest_density > 0.0
                    && artificial_pressure.is_finite()
                    && *artificial_pressure >= 0.0
                    && xsph_smoothing.is_finite()
                    && (0.0..=1.0).contains(xsph_smoothing)
                    && body_push_speed.is_finite()
                    && *body_push_speed >= 0.0
                    && density.is_finite()
                    && *density > 0.0
                    && viscosity.is_finite()
                    && *viscosity >= 0.0
            }
            Material::Gas {
                density,
                diffusivity,
                extinction,
                dissipation,
                compressibility,
                ..
            } => {
                density.is_finite()
                    && *density > 0.0
                    && diffusivity.is_finite()
                    && *diffusivity >= 0.0
                    && extinction.is_finite()
                    && *extinction >= 0.0
                    && dissipation.is_finite()
                    && *dissipation >= 0.0
                    && compressibility.is_finite()
                    && (0.0..=1.0).contains(compressibility)
            }
        }
    }

    /// Returns the material represented by a valid registered identifier
    pub fn get(&self, identifier: MaterialIdentifier) -> Option<&Material> {
        let index: usize = identifier.index() as usize;
        match identifier.form_checked()? {
            MaterialForm::CellularStatic => self.cellular_statics.get(index),
            MaterialForm::CellularDynamic => self.cellular_dynamics.get(index),
            MaterialForm::Fluid => self.fluids.get(index),
            MaterialForm::Gas => self.gases.get(index),
        }
    }

    /// Iterates over every registered material and its assigned identifier
    pub fn iter(&self) -> impl Iterator<Item = (MaterialIdentifier, &Material)> {
        self.cellular_statics
            .iter()
            .enumerate()
            .map(|(index, material)| {
                (
                    MaterialIdentifier::new(MaterialForm::CellularStatic, index as u32),
                    material,
                )
            })
            .chain(
                self.cellular_dynamics
                    .iter()
                    .enumerate()
                    .map(|(index, material)| {
                        (
                            MaterialIdentifier::new(MaterialForm::CellularDynamic, index as u32),
                            material,
                        )
                    }),
            )
            .chain(self.fluids.iter().enumerate().map(|(index, material)| {
                (
                    MaterialIdentifier::new(MaterialForm::Fluid, index as u32),
                    material,
                )
            }))
            .chain(self.gases.iter().enumerate().map(|(index, material)| {
                (
                    MaterialIdentifier::new(MaterialForm::Gas, index as u32),
                    material,
                )
            }))
    }

    /// Builds graphics properties for all registered materials
    pub fn build_material_graphics(&self, accelerator: &Accelerator) -> MaterialGraphics {
        MaterialGraphics::new(
            accelerator,
            self.cellular_statics
                .iter()
                .map(|material| *material.appearance())
                .collect(),
            self.cellular_dynamics
                .iter()
                .map(|material| *material.appearance())
                .collect(),
            self.fluids
                .iter()
                .map(|material| *material.appearance())
                .collect(),
            self.gases
                .iter()
                .map(|material| *material.appearance())
                .collect(),
            self.fluids
                .iter()
                .map(|material| match material {
                    Material::Fluid {
                        rest_density,
                        artificial_pressure,
                        xsph_smoothing,
                        body_push_speed,
                        friction,
                        restitution,
                        density,
                        viscosity,
                        ..
                    } => [
                        *rest_density,
                        *artificial_pressure,
                        *xsph_smoothing,
                        *body_push_speed,
                        *friction,
                        *restitution,
                        *density,
                        *viscosity,
                    ],
                    _ => unreachable!(),
                })
                .collect(),
            self.gases
                .iter()
                .map(|material| match material {
                    Material::Gas {
                        density,
                        diffusivity,
                        extinction,
                        dissipation,
                        compressibility,
                        ..
                    } => [
                        *density,
                        *diffusivity,
                        *extinction,
                        *dissipation,
                        *compressibility,
                        0.0,
                        0.0,
                        0.0,
                    ],
                    _ => unreachable!(),
                })
                .collect(),
        )
    }
}

impl Index<MaterialIdentifier> for MaterialRegistry {
    type Output = Material;

    fn index(&self, identifier: MaterialIdentifier) -> &Self::Output {
        let index: usize = identifier.index() as usize;
        match identifier.form() {
            MaterialForm::CellularStatic => &self.cellular_statics[index],
            MaterialForm::CellularDynamic => &self.cellular_dynamics[index],
            MaterialForm::Fluid => &self.fluids[index],
            MaterialForm::Gas => &self.gases[index],
        }
    }
}
