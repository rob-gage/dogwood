// Copyright Rob Gage 2026

use super::{
    CompiledMaterialReaction, CompiledMaterialReactionProduct, CompiledMaterialReactionReactant,
    Material, MaterialForm, MaterialIdentifier, MaterialReaction, MaterialReference,
    MaterialThermalProperties, MaterialThermalTransition,
};
use engine_compute::Accelerator;
use engine_graphics::{Color, MaterialAppearance, MaterialGraphics};
use std::{collections::BTreeMap, io, ops::Index};

/// Registers `Material`s to `MaterialIdentifier`s
pub struct MaterialRegistry {
    /// The `Material::CellularStatic`s in this `MaterialRegistry`
    cellular_statics: Vec<Material>,
    /// The `Material::CellularDynamic`s in this `MaterialRegistry`
    cellular_dynamics: Vec<Material>,
    /// The `Material::Fluid`s in this `MaterialRegistry`
    fluids: Vec<Material>,
    /// The `Material::Gas`s in this `MaterialRegistry`
    gases: Vec<Material>,
    thermal: Vec<MaterialThermalProperties>,
    tags: BTreeMap<String, Vec<MaterialIdentifier>>,
    reactions: Vec<CompiledMaterialReaction>,
    reaction_selector_members: Vec<MaterialIdentifier>,
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
        if let (Some(cold), Some(hot)) = (&properties.cold_transition, &properties.hot_transition) {
            if cold.threshold_temperature >= hot.threshold_temperature {
                return Err("Cold transition must precede hot transition".into());
            }
        }
        Ok(())
    }

    /// Returns whether all simulation properties of a material are valid
    fn material_is_valid(material: &Material) -> bool {
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

    /// Reads a `MaterialRegistry` from its binary representation
    pub fn deserialize<R: io::Read>(reader: &mut R) -> Result<Self, io::Error> {
        let mut magic: [u8; 8] = [0; 8];
        reader.read_exact(&mut magic)?;
        if &magic != b"dogwoodm" {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid material registry header",
            ));
        }
        // read each material form in identifier-index order
        let cellular_statics = Self::deserialize_form(reader, MaterialForm::CellularStatic)?;
        let cellular_dynamics = Self::deserialize_form(reader, MaterialForm::CellularDynamic)?;
        let fluids = Self::deserialize_form(reader, MaterialForm::Fluid)?;
        let gases = Self::deserialize_optional_form(reader, MaterialForm::Gas)?;
        for material in &cellular_statics {
            if let Material::CellularStatic {
                debris_material: Some(identifier),
                ..
            } = material
            {
                let valid = match identifier.form_checked() {
                    Some(MaterialForm::CellularStatic) => {
                        cellular_statics.get(identifier.index() as usize).is_some()
                    }
                    Some(MaterialForm::CellularDynamic) => {
                        cellular_dynamics.get(identifier.index() as usize).is_some()
                    }
                    Some(MaterialForm::Fluid) => fluids.get(identifier.index() as usize).is_some(),
                    Some(MaterialForm::Gas) => gases.get(identifier.index() as usize).is_some(),
                    None => false,
                };
                if !valid {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Static material debris identifier is not registered",
                    ));
                }
            }
        }
        let mut registry = Self {
            cellular_statics,
            cellular_dynamics,
            fluids,
            gases,
            thermal: Vec::new(),
            tags: BTreeMap::new(),
            reactions: Vec::new(),
            reaction_selector_members: Vec::new(),
        };
        registry.thermal =
            vec![MaterialThermalProperties::default(); registry.material_count() as usize];
        registry.deserialize_metadata(reader)?;
        Ok(registry)
    }

    /// Writes this `MaterialRegistry` in identifier-index order
    pub fn serialize<W: io::Write>(&self, writer: &mut W) -> Result<(), io::Error> {
        writer.write_all(b"dogwoodm")?;
        Self::serialize_form(writer, &self.cellular_statics)?;
        Self::serialize_form(writer, &self.cellular_dynamics)?;
        Self::serialize_form(writer, &self.fluids)?;
        Self::serialize_form(writer, &self.gases)?;
        writer.write_all(b"dwmtmeta")?;
        writer.write_all(&2u32.to_le_bytes())?;
        writer.write_all(&self.material_count().to_le_bytes())?;
        for value in &self.thermal {
            writer.write_all(&value.conductivity.to_bits().to_le_bytes())?;
            writer.write_all(&value.specific_heat_capacity.to_bits().to_le_bytes())?;
            writer.write_all(
                &value
                    .default_temperature
                    .map(f32::to_bits)
                    .unwrap_or(u32::MAX)
                    .to_le_bytes(),
            )?;
            for transition in [&value.cold_transition, &value.hot_transition] {
                writer.write_all(
                    &transition
                        .as_ref()
                        .map(|t| t.threshold_temperature.to_bits())
                        .unwrap_or(u32::MAX)
                        .to_le_bytes(),
                )?;
                writer.write_all(
                    &transition
                        .as_ref()
                        .map(|t| t.target.as_u32())
                        .unwrap_or(0)
                        .to_le_bytes(),
                )?;
                writer.write_all(
                    &transition
                        .as_ref()
                        .map(|t| t.yield_rate.to_bits())
                        .unwrap_or(0)
                        .to_le_bytes(),
                )?;
                writer.write_all(
                    &transition
                        .as_ref()
                        .map(|t| t.latent_energy.to_bits())
                        .unwrap_or(0)
                        .to_le_bytes(),
                )?;
            }
        }
        writer.write_all(&(self.tags.len() as u32).to_le_bytes())?;
        for (name, members) in &self.tags {
            writer.write_all(&(name.len() as u32).to_le_bytes())?;
            writer.write_all(name.as_bytes())?;
            writer.write_all(&(members.len() as u32).to_le_bytes())?;
            for member in members {
                writer.write_all(&member.as_u32().to_le_bytes())?;
            }
        }
        writer.write_all(&(self.reaction_selector_members.len() as u32).to_le_bytes())?;
        for member in &self.reaction_selector_members {
            writer.write_all(&member.as_u32().to_le_bytes())?;
        }
        writer.write_all(&(self.reactions.len() as u32).to_le_bytes())?;
        for rule in &self.reactions {
            let words = [
                rule.reactants[0].member_offset,
                rule.reactants[0].member_count,
                rule.reactants[0].amount_bits,
                rule.reactants[0].present,
                rule.reactants[1].member_offset,
                rule.reactants[1].member_count,
                rule.reactants[1].amount_bits,
                rule.reactants[1].present,
                rule.products[0].material,
                rule.products[0].amount_bits,
                rule.products[0].present,
                0,
                rule.products[1].material,
                rule.products[1].amount_bits,
                rule.products[1].present,
                0,
                rule.minimum_temperature.to_bits(),
                rule.maximum_temperature.to_bits(),
                rule.minimum_pressure.to_bits(),
                rule.maximum_pressure.to_bits(),
                rule.minimum_air.to_bits(),
                rule.maximum_air.to_bits(),
                rule.maximum_extent_per_tick.to_bits(),
                rule.thermal_energy.to_bits(),
                rule.pressure_output.to_bits(),
                rule.priority as u32,
                rule.authoring_order,
            ];
            for word in words {
                writer.write_all(&word.to_le_bytes())?;
            }
        }
        Ok(())
    }

    fn deserialize_metadata<R: io::Read>(&mut self, reader: &mut R) -> Result<(), io::Error> {
        let mut magic = [0; 8];
        let read = reader.read(&mut magic)?;
        if read == 0 {
            return Ok(());
        }
        if read != 8 || &magic != b"dwmtmeta" {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid material metadata extension",
            ));
        }
        let version = Self::read_u32(reader)?;
        if version != 1 && version != 2 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid material metadata extension",
            ));
        }
        if Self::read_u32(reader)? != self.material_count() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Material metadata count mismatch",
            ));
        }
        let mut metadata = BTreeMap::new();
        for index in 0..self.material_count() {
            let conductivity = f32::from_bits(Self::read_u32(reader)?);
            let specific_heat_capacity = f32::from_bits(Self::read_u32(reader)?);
            let default = Self::read_u32(reader)?;
            let transition =
                |reader: &mut R| -> Result<Option<MaterialThermalTransition>, io::Error> {
                    let threshold = Self::read_u32(reader)?;
                    let target = MaterialIdentifier::from_u32(Self::read_u32(reader)?);
                    let yield_rate = f32::from_bits(Self::read_u32(reader)?);
                    let latent_energy = f32::from_bits(Self::read_u32(reader)?);
                    Ok(
                        (threshold != u32::MAX).then_some(MaterialThermalTransition {
                            threshold_temperature: f32::from_bits(threshold),
                            target,
                            yield_rate,
                            latent_energy,
                        }),
                    )
                };
            metadata.insert(
                self.identifier_from_dense_index(index).unwrap(),
                MaterialThermalProperties {
                    conductivity,
                    specific_heat_capacity,
                    default_temperature: (default != u32::MAX).then_some(f32::from_bits(default)),
                    cold_transition: transition(reader)?,
                    hot_transition: transition(reader)?,
                },
            );
        }
        let count = Self::read_u32(reader)?;
        if count > 1_000_000 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Too many tags"));
        }
        let mut tags = BTreeMap::new();
        for _ in 0..count {
            let len = Self::read_u32(reader)? as usize;
            if len > 4096 {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "Tag too long"));
            }
            let mut bytes = vec![0; len];
            reader.read_exact(&mut bytes)?;
            let name = String::from_utf8(bytes)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            let members = Self::read_u32(reader)?;
            if members > self.material_count() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Too many tag members",
                ));
            }
            let mut values = Vec::with_capacity(members as usize);
            for _ in 0..members {
                values.push(MaterialIdentifier::from_u32(Self::read_u32(reader)?));
            }
            if tags.insert(name, values).is_some() {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "Duplicate tag"));
            }
        }
        self.set_compiled_metadata(metadata, tags, Vec::new())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        if version == 2 {
            let member_count = Self::read_u32(reader)? as usize;
            if member_count > self.material_count() as usize * 1024 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Too many reaction selector members",
                ));
            }
            let mut members = Vec::with_capacity(member_count);
            for _ in 0..member_count {
                let id = MaterialIdentifier::from_u32(Self::read_u32(reader)?);
                if self.get(id).is_none() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Invalid reaction selector member",
                    ));
                }
                members.push(id);
            }
            let reaction_count = Self::read_u32(reader)? as usize;
            if reaction_count > 65536 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Too many reactions",
                ));
            }
            let mut reactions = Vec::with_capacity(reaction_count);
            for _ in 0..reaction_count {
                let mut words = [0u32; CompiledMaterialReaction::WORD_COUNT];
                for word in &mut words {
                    *word = Self::read_u32(reader)?;
                }
                let reactant =
                    |offset, count, amount_bits, present| CompiledMaterialReactionReactant {
                        member_offset: offset,
                        member_count: count,
                        amount_bits,
                        present,
                    };
                let product = |material, amount_bits, present| CompiledMaterialReactionProduct {
                    material,
                    amount_bits,
                    present,
                    _padding: 0,
                };
                let rule = CompiledMaterialReaction {
                    reactants: [
                        reactant(words[0], words[1], words[2], words[3]),
                        reactant(words[4], words[5], words[6], words[7]),
                    ],
                    products: [
                        product(words[8], words[9], words[10]),
                        product(words[12], words[13], words[14]),
                    ],
                    minimum_temperature: f32::from_bits(words[16]),
                    maximum_temperature: f32::from_bits(words[17]),
                    minimum_pressure: f32::from_bits(words[18]),
                    maximum_pressure: f32::from_bits(words[19]),
                    minimum_air: f32::from_bits(words[20]),
                    maximum_air: f32::from_bits(words[21]),
                    maximum_extent_per_tick: f32::from_bits(words[22]),
                    thermal_energy: f32::from_bits(words[23]),
                    pressure_output: f32::from_bits(words[24]),
                    priority: words[25] as i32,
                    authoring_order: words[26],
                };
                for reactant in &rule.reactants {
                    if reactant.present > 1
                        || reactant
                            .member_offset
                            .checked_add(reactant.member_count)
                            .is_none_or(|end| end as usize > members.len())
                        || (reactant.present == 1
                            && (!f32::from_bits(reactant.amount_bits).is_finite()
                                || f32::from_bits(reactant.amount_bits) <= 0.0))
                    {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "Invalid compiled reaction reactant",
                        ));
                    }
                }
                for product in &rule.products {
                    if product.present > 1
                        || (product.present == 1
                            && (self
                                .get(MaterialIdentifier::from_u32(product.material))
                                .is_none()
                                || !f32::from_bits(product.amount_bits).is_finite()
                                || f32::from_bits(product.amount_bits) <= 0.0))
                    {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "Invalid compiled reaction product",
                        ));
                    }
                }
                reactions.push(rule);
            }
            self.reaction_selector_members = members;
            self.reactions = reactions;
        }
        Ok(())
    }

    /// Reads an optional trailing material form, preserving three-form registries
    fn deserialize_optional_form<R: io::Read>(
        reader: &mut R,
        form: MaterialForm,
    ) -> Result<Vec<Material>, io::Error> {
        let mut first_count_byte: [u8; 1] = [0];
        if reader.read(&mut first_count_byte)? == 0 {
            return Ok(Vec::new());
        }
        let mut remaining_count_bytes: [u8; 3] = [0; 3];
        reader.read_exact(&mut remaining_count_bytes)?;
        let count: u32 = u32::from_le_bytes([
            first_count_byte[0],
            remaining_count_bytes[0],
            remaining_count_bytes[1],
            remaining_count_bytes[2],
        ]);
        Self::deserialize_form_count(reader, form, count)
    }

    /// Reads all materials belonging to one material form
    fn deserialize_form<R: io::Read>(
        reader: &mut R,
        form: MaterialForm,
    ) -> Result<Vec<Material>, io::Error> {
        let count: u32 = Self::read_u32(reader)?;
        Self::deserialize_form_count(reader, form, count)
    }

    fn deserialize_form_count<R: io::Read>(
        reader: &mut R,
        form: MaterialForm,
        count: u32,
    ) -> Result<Vec<Material>, io::Error> {
        let mut materials: Vec<Material> = Vec::with_capacity(count as usize);
        for _ in 0..count {
            // decode the owned name first so registries can be loaded from disk.
            let name_length: u32 = Self::read_u32(reader)?;
            let mut name_bytes: Vec<u8> = vec![0; name_length as usize];
            reader.read_exact(&mut name_bytes)?;
            let name: String = String::from_utf8(name_bytes)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
            // decode the four packed RGBA colors comprising the appearance.
            let color_freezing: Color = Self::read_color(reader)?;
            let color_melting: Color = Self::read_color(reader)?;
            let radiance_freezing: Color = Self::read_color(reader)?;
            let radiance_melting: Color = Self::read_color(reader)?;
            let variation: [f32; 4] = Self::read_f32_array(reader)?;
            let color_influence: [f32; 4] = Self::read_f32_array(reader)?;
            let radiance_influence: [f32; 4] = Self::read_f32_array(reader)?;
            let graphics: MaterialAppearance = MaterialAppearance::new(
                color_freezing,
                color_melting,
                radiance_freezing,
                radiance_melting,
            )
            .with_variation(variation)
            .with_color_influence(color_influence)
            .with_radiance_influence(radiance_influence);
            let material = match form {
                MaterialForm::CellularStatic => {
                    let mass = f32::from_bits(Self::read_u32(reader)?);
                    let pressure_ignore_threshold = f32::from_bits(Self::read_u32(reader)?);
                    let default_integrity = f32::from_bits(Self::read_u32(reader)?);
                    let minimum_rigid_body_cell_count = Self::read_u32(reader)?;
                    let debris_identifier = MaterialIdentifier::from_u32(Self::read_u32(reader)?);
                    let debris_material = (debris_identifier != MaterialIdentifier::NULL)
                        .then_some(debris_identifier);
                    let debris_yield_rate = f32::from_bits(Self::read_u32(reader)?);
                    let pressure_transmission = f32::from_bits(Self::read_u32(reader)?);
                    let friction = f32::from_bits(Self::read_u32(reader)?);
                    let restitution = f32::from_bits(Self::read_u32(reader)?);
                    Material::CellularStatic {
                        name,
                        graphics,
                        mass,
                        pressure_ignore_threshold,
                        default_integrity,
                        minimum_rigid_body_cell_count,
                        debris_material,
                        debris_yield_rate,
                        pressure_transmission,
                        friction,
                        restitution,
                    }
                }
                MaterialForm::CellularDynamic => Material::CellularDynamic {
                    name,
                    graphics,
                    mass: f32::from_bits(Self::read_u32(reader)?),
                    pressure_transmission: f32::from_bits(Self::read_u32(reader)?),
                    friction: f32::from_bits(Self::read_u32(reader)?),
                    restitution: f32::from_bits(Self::read_u32(reader)?),
                },
                MaterialForm::Fluid => Material::Fluid {
                    name,
                    graphics,
                    pressure_transmission: f32::from_bits(Self::read_u32(reader)?),
                    friction: f32::from_bits(Self::read_u32(reader)?),
                    restitution: f32::from_bits(Self::read_u32(reader)?),
                    rest_density: f32::from_bits(Self::read_u32(reader)?),
                    artificial_pressure: f32::from_bits(Self::read_u32(reader)?),
                    xsph_smoothing: f32::from_bits(Self::read_u32(reader)?),
                    body_push_speed: f32::from_bits(Self::read_u32(reader)?),
                    density: f32::from_bits(Self::read_u32(reader)?),
                    viscosity: f32::from_bits(Self::read_u32(reader)?),
                },
                MaterialForm::Gas => Material::Gas {
                    name,
                    graphics,
                    density: f32::from_bits(Self::read_u32(reader)?),
                    diffusivity: f32::from_bits(Self::read_u32(reader)?),
                    extinction: f32::from_bits(Self::read_u32(reader)?),
                    dissipation: f32::from_bits(Self::read_u32(reader)?),
                    compressibility: f32::from_bits(Self::read_u32(reader)?),
                },
            };
            if !Self::material_is_valid(&material) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Invalid material properties",
                ));
            }
            materials.push(material);
        }
        Ok(materials)
    }

    /// Writes all materials belonging to one material form
    fn serialize_form<W: io::Write>(
        writer: &mut W,
        materials: &[Material],
    ) -> Result<(), io::Error> {
        let count: u32 = materials.len().try_into().map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "Too many registered materials")
        })?;
        writer.write_all(&count.to_le_bytes())?;
        for material in materials {
            // store the name as length-prefixed UTF-8.
            let name: &[u8] = material.name().as_bytes();
            let name_length: u32 = name.len().try_into().map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidInput, "Material name is too long")
            })?;
            writer.write_all(&name_length.to_le_bytes())?;
            writer.write_all(name)?;
            // preserve the appearance's exact packed Accelerator values.
            let graphics: MaterialAppearance = *material.appearance();
            for value in graphics.accelerator_data()[..4].iter() {
                writer.write_all(&value.to_le_bytes())?;
            }
            for value in [
                graphics.variation(),
                graphics.color_influence(),
                graphics.radiance_influence(),
            ]
            .into_iter()
            .flatten()
            {
                writer.write_all(&value.to_bits().to_le_bytes())?;
            }
            match material {
                Material::CellularStatic {
                    mass,
                    pressure_ignore_threshold,
                    default_integrity,
                    minimum_rigid_body_cell_count,
                    debris_material,
                    debris_yield_rate,
                    pressure_transmission,
                    friction,
                    restitution,
                    ..
                } => {
                    writer.write_all(&mass.to_bits().to_le_bytes())?;
                    writer.write_all(&pressure_ignore_threshold.to_bits().to_le_bytes())?;
                    writer.write_all(&default_integrity.to_bits().to_le_bytes())?;
                    writer.write_all(&minimum_rigid_body_cell_count.to_le_bytes())?;
                    writer.write_all(
                        &debris_material
                            .unwrap_or(MaterialIdentifier::NULL)
                            .as_u32()
                            .to_le_bytes(),
                    )?;
                    writer.write_all(&debris_yield_rate.to_bits().to_le_bytes())?;
                    writer.write_all(&pressure_transmission.to_bits().to_le_bytes())?;
                    writer.write_all(&friction.to_bits().to_le_bytes())?;
                    writer.write_all(&restitution.to_bits().to_le_bytes())?;
                }
                Material::CellularDynamic {
                    mass,
                    pressure_transmission,
                    friction,
                    restitution,
                    ..
                } => {
                    writer.write_all(&mass.to_bits().to_le_bytes())?;
                    writer.write_all(&pressure_transmission.to_bits().to_le_bytes())?;
                    writer.write_all(&friction.to_bits().to_le_bytes())?;
                    writer.write_all(&restitution.to_bits().to_le_bytes())?;
                }
                Material::Fluid {
                    pressure_transmission,
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
                    writer.write_all(&pressure_transmission.to_bits().to_le_bytes())?;
                    writer.write_all(&friction.to_bits().to_le_bytes())?;
                    writer.write_all(&restitution.to_bits().to_le_bytes())?;
                    writer.write_all(&rest_density.to_bits().to_le_bytes())?;
                    writer.write_all(&artificial_pressure.to_bits().to_le_bytes())?;
                    writer.write_all(&xsph_smoothing.to_bits().to_le_bytes())?;
                    writer.write_all(&body_push_speed.to_bits().to_le_bytes())?;
                    writer.write_all(&density.to_bits().to_le_bytes())?;
                    writer.write_all(&viscosity.to_bits().to_le_bytes())?;
                }
                Material::Gas {
                    density,
                    diffusivity,
                    extinction,
                    dissipation,
                    compressibility,
                    ..
                } => {
                    writer.write_all(&density.to_bits().to_le_bytes())?;
                    writer.write_all(&diffusivity.to_bits().to_le_bytes())?;
                    writer.write_all(&extinction.to_bits().to_le_bytes())?;
                    writer.write_all(&dissipation.to_bits().to_le_bytes())?;
                    writer.write_all(&compressibility.to_bits().to_le_bytes())?;
                }
            }
        }
        Ok(())
    }

    /// Reads one little-endian `u32`
    fn read_u32<R: io::Read>(reader: &mut R) -> Result<u32, io::Error> {
        let mut bytes: [u8; 4] = [0; 4];
        reader.read_exact(&mut bytes)?;
        Ok(u32::from_le_bytes(bytes))
    }

    /// Reads one packed RGBA color
    fn read_color<R: io::Read>(reader: &mut R) -> Result<Color, io::Error> {
        let bytes: [u8; 4] = Self::read_u32(reader)?.to_le_bytes();
        Ok(Color::new_rgba(bytes[0], bytes[1], bytes[2], bytes[3]))
    }

    /// Reads four little-endian `f32` values
    fn read_f32_array<R: io::Read>(reader: &mut R) -> Result<[f32; 4], io::Error> {
        let mut values: [f32; 4] = [0.0; 4];
        for value in &mut values {
            *value = f32::from_bits(Self::read_u32(reader)?);
        }
        Ok(values)
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
