// Copyright Rob Gage 2026

use super::{Material, MaterialIdentifier, MaterialRegistry};
use engine_graphics::MaterialAppearance;
use std::io;

impl MaterialRegistry {
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
            let words: [u32; 27] = [
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
}
