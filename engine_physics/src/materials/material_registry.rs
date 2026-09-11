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

    /// Returns the material represented by a valid registered identifier
    pub fn get(&self, identifier: MaterialIdentifier) -> Option<&Material> {
        let index: usize = identifier.index() as usize;
        match identifier.form_checked()? {
            MaterialForm::CellularStatic => self.cellular_statics.get(index),
            MaterialForm::CellularDynamic => self.cellular_dynamics.get(index),
            MaterialForm::Fluid => self.fluids.get(index),
        }
    }

    /// Builds graphics properties for all registered materials
    pub fn build_material_graphics(&self, accelerator: &Accelerator) -> MaterialGraphics {
        MaterialGraphics::new(
            accelerator,
            self.cellular_statics.iter().map(|material| *material.appearance()).collect(),
            self.cellular_dynamics.iter().map(|material| *material.appearance()).collect(),
            self.fluids.iter().map(|material| *material.appearance()).collect(),
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
        // read each material form in identifier-index order.
        let cellular_statics: Vec<Material> = Self::deserialize_form(
            reader,
            |name: String, graphics: MaterialAppearance| Material::CellularStatic {
                name,
                graphics,
            },
        )?;
        let cellular_dynamics: Vec<Material> = Self::deserialize_form(
            reader,
            |name: String, graphics: MaterialAppearance| Material::CellularDynamic {
                name,
                graphics,
            },
        )?;
        let fluids: Vec<Material> = Self::deserialize_form(
            reader,
            |name: String, graphics: MaterialAppearance| Material::Fluid { name, graphics },
        )?;
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
        material: impl Fn(String, MaterialAppearance) -> Material,
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
            materials.push(material(name, graphics));
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
