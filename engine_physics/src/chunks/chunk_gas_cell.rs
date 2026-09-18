// Copyright Rob Gage 2026

use crate::{
    materials::{MaterialForm, MaterialIdentifier},
    tiles::{CellCoordinates, TileCoordinates},
};
use std::io;

/// Sparse authoritative gas state for one nonresident world cell
#[derive(Clone)]
pub struct ChunkGasCell {
    /// The world cell represented by this record
    pub coordinates: CellCoordinates,
    /// Shared gas-mixture velocity in cells per second
    pub velocity: [f32; 2],
    /// Gas-mixture temperature in the engine's material temperature units.
    pub temperature: f32,
    /// Non-negligible species concentrations in this cell
    pub species: Vec<(MaterialIdentifier, f32)>,
}

impl ChunkGasCell {
    /// Returns the world tile containing this cell
    pub const fn tile_coordinates(&self) -> TileCoordinates {
        self.coordinates.tile_coordinates()
    }

    /// Reads one sparse dormant gas cell
    pub fn deserialize<R: io::Read>(reader: &mut R) -> Result<Self, io::Error> {
        let coordinates: CellCoordinates = CellCoordinates {
            x: crate::binary_reader::read_u32(reader)? as i32,
            y: crate::binary_reader::read_u32(reader)? as i32,
        };
        let velocity: [f32; 2] = [
            f32::from_bits(crate::binary_reader::read_u32(reader)?),
            f32::from_bits(crate::binary_reader::read_u32(reader)?),
        ];
        let temperature: f32 = f32::from_bits(crate::binary_reader::read_u32(reader)?);
        let count: usize = crate::binary_reader::read_u32(reader)? as usize;
        let mut species: Vec<(MaterialIdentifier, f32)> = Vec::new();
        species.try_reserve_exact(count).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "Dormant gas species count is too large",
            )
        })?;
        for _ in 0..count {
            species.push((
                MaterialIdentifier::from_u32(crate::binary_reader::read_u32(reader)?),
                f32::from_bits(crate::binary_reader::read_u32(reader)?),
            ));
        }
        let cell: Self = Self {
            coordinates,
            velocity,
            temperature,
            species,
        };
        cell.validate()?;
        Ok(cell)
    }

    pub fn deserialize_legacy<R: io::Read>(reader: &mut R) -> Result<Self, io::Error> {
        let coordinates: CellCoordinates = CellCoordinates {
            x: crate::binary_reader::read_u32(reader)? as i32,
            y: crate::binary_reader::read_u32(reader)? as i32,
        };
        let velocity: [f32; 2] = [
            f32::from_bits(crate::binary_reader::read_u32(reader)?),
            f32::from_bits(crate::binary_reader::read_u32(reader)?),
        ];
        let count: usize = crate::binary_reader::read_u32(reader)? as usize;
        let mut species: Vec<(MaterialIdentifier, f32)> = Vec::new();
        species.try_reserve_exact(count).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "Dormant gas species count is too large",
            )
        })?;
        for _ in 0..count {
            species.push((
                MaterialIdentifier::from_u32(crate::binary_reader::read_u32(reader)?),
                f32::from_bits(crate::binary_reader::read_u32(reader)?),
            ));
        }
        let cell: Self = Self {
            coordinates,
            velocity,
            temperature: f32::NAN,
            species,
        };
        cell.validate_legacy()?;
        Ok(cell)
    }

    /// Writes one sparse dormant gas cell
    pub fn serialize<W: io::Write>(&self, writer: &mut W) -> Result<(), io::Error> {
        self.validate()?;
        writer.write_all(&self.coordinates.x.to_le_bytes())?;
        writer.write_all(&self.coordinates.y.to_le_bytes())?;
        for velocity in self.velocity {
            writer.write_all(&velocity.to_bits().to_le_bytes())?;
        }
        writer.write_all(&self.temperature.to_bits().to_le_bytes())?;
        let count: u32 = self.species.len().try_into().map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidData, "Too many dormant gas species")
        })?;
        writer.write_all(&count.to_le_bytes())?;
        for (identifier, concentration) in &self.species {
            writer.write_all(&identifier.as_u32().to_le_bytes())?;
            writer.write_all(&concentration.to_bits().to_le_bytes())?;
        }
        Ok(())
    }

    pub(crate) fn validate(&self) -> Result<(), io::Error> {
        let valid: bool = self.velocity.into_iter().all(f32::is_finite)
            && !self.species.is_empty()
            && self.temperature.is_finite()
            && self.temperature >= 0.0
            && self
                .species
                .iter()
                .enumerate()
                .all(|(index, (identifier, concentration))| {
                    identifier.form_checked() == Some(MaterialForm::Gas)
                        && concentration.is_finite()
                        && *concentration > 0.0
                        && !self.species[..index]
                            .iter()
                            .any(|(other, _)| other == identifier)
                });
        if valid {
            return Ok(());
        }
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid dormant gas cell",
        ))
    }

    fn validate_legacy(&self) -> Result<(), io::Error> {
        if self.velocity.into_iter().all(f32::is_finite) && !self.species.is_empty() {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid dormant gas cell",
            ))
        }
    }
}
