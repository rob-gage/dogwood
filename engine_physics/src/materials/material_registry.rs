// Copyright Rob Gage 2026

use super::{
    Material,
    MaterialForm,
    MaterialIdentifier
};
use engine_compute::Accelerator;
use engine_graphics::{
    Color,
    MaterialAppearance,
    MaterialGraphics,
};
use std::{
    io,
    ops::Index,
};

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
        assert!(Self::material_is_valid(&material));
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

    /// Returns whether all simulation properties of a material are valid
    fn material_is_valid(material: &Material) -> bool {
        match material {
            Material::CellularStatic { pressure_transmission, friction, restitution, .. } =>
                pressure_transmission.is_finite() && (0.0..=1.0).contains(pressure_transmission) &&
                friction.is_finite() && (0.0..=1.0).contains(friction) &&
                restitution.is_finite() && (0.0..=1.0).contains(restitution),
            Material::CellularDynamic { mass, pressure_transmission, friction, restitution, .. } =>
                mass.is_finite() && *mass > 0.0 && pressure_transmission.is_finite() &&
                    (0.0..=1.0).contains(pressure_transmission) && friction.is_finite() &&
                    (0.0..=1.0).contains(friction) && restitution.is_finite() &&
                    (0.0..=1.0).contains(restitution),
            Material::Fluid { friction, restitution, rest_density, artificial_pressure,
                xsph_smoothing, body_push_speed, density, viscosity, .. } =>
                friction.is_finite() && (0.0..=1.0).contains(friction) &&
                restitution.is_finite() && (0.0..=1.0).contains(restitution) &&
                rest_density.is_finite() && *rest_density > 0.0 &&
                artificial_pressure.is_finite() && *artificial_pressure >= 0.0 &&
                xsph_smoothing.is_finite() && (0.0..=1.0).contains(xsph_smoothing) &&
                body_push_speed.is_finite() && *body_push_speed >= 0.0 &&
                density.is_finite() && *density > 0.0 &&
                viscosity.is_finite() && *viscosity >= 0.0,
        }
    }

    /// Returns the material represented by a valid registered identifier
    pub fn get(&self, identifier: MaterialIdentifier) -> Option<&Material> {
        let index: usize = identifier.index() as usize;
        match identifier.form_checked()? {
            MaterialForm::CellularStatic => self.cellular_statics.get(index),
            MaterialForm::CellularDynamic => self.cellular_dynamics.get(index),
            MaterialForm::Fluid => self.fluids.get(index),
        }
    }

    /// Iterates over every registered material and its assigned identifier
    pub fn iter(&self) -> impl Iterator<Item = (MaterialIdentifier, &Material)> {
        self.cellular_statics.iter().enumerate().map(|(index, material)| (
            MaterialIdentifier::new(MaterialForm::CellularStatic, index as u32),
            material,
        )).chain(self.cellular_dynamics.iter().enumerate().map(|(index, material)| (
            MaterialIdentifier::new(MaterialForm::CellularDynamic, index as u32),
            material,
        ))).chain(self.fluids.iter().enumerate().map(|(index, material)| (
            MaterialIdentifier::new(MaterialForm::Fluid, index as u32),
            material,
        )))
    }

    /// Builds graphics properties for all registered materials
    pub fn build_material_graphics(&self, accelerator: &Accelerator) -> MaterialGraphics {
        MaterialGraphics::new(
            accelerator,
            self.cellular_statics.iter().map(|material| *material.appearance()).collect(),
            self.cellular_dynamics.iter().map(|material| *material.appearance()).collect(),
            self.fluids.iter().map(|material| *material.appearance()).collect(),
            self.fluids.iter().map(|material| match material {
                Material::Fluid { rest_density, artificial_pressure, xsph_smoothing,
                    body_push_speed, friction, restitution, density, viscosity, .. } => [*rest_density,
                    *artificial_pressure, *xsph_smoothing, *body_push_speed, *friction,
                    *restitution, *density, *viscosity],
                _ => unreachable!(),
            }).collect(),
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
        for material in &cellular_statics {
            if let Material::CellularStatic { debris_material: Some(identifier), .. } = material {
                if identifier.form_checked() != Some(MaterialForm::CellularDynamic) ||
                        !matches!(
                            cellular_dynamics.get(identifier.index() as usize),
                            Some(Material::CellularDynamic { .. })
                        ) {
                            return Err(io::Error::new(io::ErrorKind::InvalidData,
                                "Static material debris identifier is not a dynamic material"));
                        }
            }
        }
        Ok(Self { cellular_statics, cellular_dynamics, fluids })
    }

    /// Writes this `MaterialRegistry` in identifier-index order
    pub fn serialize<W: io::Write>(&self, writer: &mut W) -> Result<(), io::Error> {
        writer.write_all(b"dogwoodm")?;
        Self::serialize_form(writer, &self.cellular_statics)?;
        Self::serialize_form(writer, &self.cellular_dynamics)?;
        Self::serialize_form(writer, &self.fluids)
    }

    /// Reads all materials belonging to one material form
    fn deserialize_form<R: io::Read>(
        reader: &mut R,
        form: MaterialForm,
    ) -> Result<Vec<Material>, io::Error> {
        let count: u32 = Self::read_u32(reader)?;
        let mut materials: Vec<Material> = Vec::with_capacity(count as usize);
        for _ in 0..count {
            // decode the owned name first so registries can be loaded from disk.
            let name_length: u32 = Self::read_u32(reader)?;
            let mut name_bytes: Vec<u8> = vec![0; name_length as usize];
            reader.read_exact(&mut name_bytes)?;
            let name: String = String::from_utf8(name_bytes).map_err(|error| {
                io::Error::new(io::ErrorKind::InvalidData, error)
            })?;
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
            ).with_variation(variation)
                .with_color_influence(color_influence)
                .with_radiance_influence(radiance_influence);
            let material = match form {
                MaterialForm::CellularStatic => {
                    let pressure_ignore_threshold = f32::from_bits(Self::read_u32(reader)?);
                    let default_integrity = f32::from_bits(Self::read_u32(reader)?);
                    let debris_identifier = MaterialIdentifier::from_u32(Self::read_u32(reader)?);
                    let debris_material = (debris_identifier != MaterialIdentifier::NULL)
                        .then_some(debris_identifier);
                    let debris_yield_rate = f32::from_bits(Self::read_u32(reader)?);
                    let pressure_transmission = f32::from_bits(Self::read_u32(reader)?);
                    let friction = f32::from_bits(Self::read_u32(reader)?);
                    let restitution = f32::from_bits(Self::read_u32(reader)?);
                    Material::CellularStatic { name, graphics, pressure_ignore_threshold,
                        default_integrity, debris_material, debris_yield_rate, pressure_transmission,
                        friction, restitution }
                }
                MaterialForm::CellularDynamic => Material::CellularDynamic { name, graphics,
                    mass: f32::from_bits(Self::read_u32(reader)?),
                    pressure_transmission: f32::from_bits(Self::read_u32(reader)?),
                    friction: f32::from_bits(Self::read_u32(reader)?),
                    restitution: f32::from_bits(Self::read_u32(reader)?),
                },
                MaterialForm::Fluid => Material::Fluid { name, graphics,
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
            // preserve the appearance's exact packed GPU values.
            let graphics: MaterialAppearance = *material.appearance();
            for value in graphics.accelerator_data()[..4].iter() {
                writer.write_all(&value.to_le_bytes())?;
            }
            for value in [
                graphics.variation(),
                graphics.color_influence(),
                graphics.radiance_influence(),
            ].into_iter().flatten() {
                writer.write_all(&value.to_bits().to_le_bytes())?;
            }
            match material {
                Material::CellularStatic { pressure_ignore_threshold, default_integrity,
                    debris_material, debris_yield_rate, pressure_transmission, friction, restitution, .. } => {
                    writer.write_all(&pressure_ignore_threshold.to_bits().to_le_bytes())?;
                    writer.write_all(&default_integrity.to_bits().to_le_bytes())?;
                    writer.write_all(&debris_material.unwrap_or(MaterialIdentifier::NULL).as_u32().to_le_bytes())?;
                    writer.write_all(&debris_yield_rate.to_bits().to_le_bytes())?;
                    writer.write_all(&pressure_transmission.to_bits().to_le_bytes())?;
                    writer.write_all(&friction.to_bits().to_le_bytes())?;
                    writer.write_all(&restitution.to_bits().to_le_bytes())?;
                }
                Material::CellularDynamic { mass, pressure_transmission, friction, restitution, .. } => {
                    writer.write_all(&mass.to_bits().to_le_bytes())?;
                    writer.write_all(&pressure_transmission.to_bits().to_le_bytes())?;
                    writer.write_all(&friction.to_bits().to_le_bytes())?;
                    writer.write_all(&restitution.to_bits().to_le_bytes())?;
                }
                Material::Fluid { pressure_transmission, friction, restitution,
                    rest_density, artificial_pressure, xsph_smoothing, body_push_speed,
                    density, viscosity, .. } => {
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
            MaterialForm::CellularStatic    => &self.cellular_statics[index],
            MaterialForm::CellularDynamic   => &self.cellular_dynamics[index],
            MaterialForm::Fluid             => &self.fluids[index],

        }
    }

}
