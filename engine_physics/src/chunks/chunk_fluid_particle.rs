// Copyright Rob Gage 2026

use crate::{
    materials::{MaterialForm, MaterialIdentifier},
    tiles::TileCoordinates,
};
use std::io;

/// Minimum authoritative state of a nonresident fluid particle
#[derive(Clone, Copy)]
pub struct ChunkFluidParticle {
    /// The fluid material represented by this particle
    pub material_identifier: MaterialIdentifier,
    /// Continuous world position in tile units
    pub position: [f32; 2],
    /// Continuous velocity in tiles per second
    pub velocity: [f32; 2],
    pub amount: f32,
    pub temperature: f32,
}

impl ChunkFluidParticle {
    /// The aligned size of the matching Accelerator particle record
    pub const GPU_SIZE: usize = 40;
    pub const SERIALIZED_SIZE: usize = 28;

    /// Returns the world tile containing this particle
    pub fn tile_coordinates(self) -> TileCoordinates {
        TileCoordinates {
            x: self.position[0].floor() as i32,
            y: self.position[1].floor() as i32,
        }
    }

    /// Reads one persistent dormant particle record
    pub fn deserialize<R: io::Read>(reader: &mut R) -> Result<Self, io::Error> {
        let material_identifier: MaterialIdentifier =
            MaterialIdentifier::from_u32(crate::binary_reader::read_u32(reader)?);
        let particle: Self = Self {
            material_identifier,
            position: [
                f32::from_bits(crate::binary_reader::read_u32(reader)?),
                f32::from_bits(crate::binary_reader::read_u32(reader)?),
            ],
            velocity: [
                f32::from_bits(crate::binary_reader::read_u32(reader)?),
                f32::from_bits(crate::binary_reader::read_u32(reader)?),
            ],
            amount: f32::from_bits(crate::binary_reader::read_u32(reader)?),
            temperature: f32::from_bits(crate::binary_reader::read_u32(reader)?),
        };
        particle.validate()?;
        Ok(particle)
    }

    pub fn deserialize_legacy<R: io::Read>(reader: &mut R) -> Result<Self, io::Error> {
        let material_identifier =
            MaterialIdentifier::from_u32(crate::binary_reader::read_u32(reader)?);
        let particle = Self {
            material_identifier,
            position: [
                f32::from_bits(crate::binary_reader::read_u32(reader)?),
                f32::from_bits(crate::binary_reader::read_u32(reader)?),
            ],
            velocity: [
                f32::from_bits(crate::binary_reader::read_u32(reader)?),
                f32::from_bits(crate::binary_reader::read_u32(reader)?),
            ],
            amount: 1.0,
            temperature: f32::NAN,
        };
        if particle.material_identifier.form_checked() != Some(MaterialForm::Fluid)
            || !particle
                .position
                .into_iter()
                .chain(particle.velocity)
                .all(f32::is_finite)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid dormant fluid particle",
            ));
        }
        Ok(particle)
    }

    /// Reads one aligned Accelerator particle record
    pub fn deserialize_gpu(bytes: &[u8]) -> Result<Self, io::Error> {
        if bytes.len() != Self::GPU_SIZE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid downloaded fluid particle size",
            ));
        }
        let particle: Self = Self {
            material_identifier: MaterialIdentifier::from_u32(crate::binary_reader::read_u32_at(
                bytes, 0,
            )),
            position: [
                f32::from_bits(crate::binary_reader::read_u32_at(bytes, 8)),
                f32::from_bits(crate::binary_reader::read_u32_at(bytes, 12)),
            ],
            velocity: [
                f32::from_bits(crate::binary_reader::read_u32_at(bytes, 16)),
                f32::from_bits(crate::binary_reader::read_u32_at(bytes, 20)),
            ],
            amount: f32::from_bits(crate::binary_reader::read_u32_at(bytes, 32)),
            temperature: f32::from_bits(crate::binary_reader::read_u32_at(bytes, 36)),
        };
        particle.validate()?;
        Ok(particle)
    }

    /// Writes one persistent dormant particle record
    pub fn serialize<W: io::Write>(&self, writer: &mut W) -> Result<(), io::Error> {
        self.validate()?;
        writer.write_all(&self.material_identifier.as_u32().to_le_bytes())?;
        for value in self.position.into_iter().chain(self.velocity) {
            writer.write_all(&value.to_bits().to_le_bytes())?;
        }
        writer.write_all(&self.amount.to_bits().to_le_bytes())?;
        writer.write_all(&self.temperature.to_bits().to_le_bytes())?;
        Ok(())
    }

    /// Appends one aligned Accelerator particle record
    pub fn serialize_gpu(&self, bytes: &mut Vec<u8>) -> Result<(), io::Error> {
        self.validate()?;
        bytes.extend_from_slice(&self.material_identifier.as_u32().to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        for value in self.position.into_iter().chain(self.velocity) {
            bytes.extend_from_slice(&value.to_bits().to_le_bytes());
        }
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&self.amount.to_bits().to_le_bytes());
        bytes.extend_from_slice(&self.temperature.to_bits().to_le_bytes());
        debug_assert_eq!(bytes.len() % Self::GPU_SIZE, 0);
        Ok(())
    }

    fn validate(&self) -> Result<(), io::Error> {
        if self.material_identifier.form_checked() != Some(MaterialForm::Fluid)
            || !self
                .position
                .into_iter()
                .chain(self.velocity)
                .all(f32::is_finite)
            || !self.amount.is_finite()
            || self.amount <= 0.0
            || !self.temperature.is_finite()
            || self.temperature < 0.0
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid dormant fluid particle",
            ));
        }
        Ok(())
    }
}
