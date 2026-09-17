// Copyright Rob Gage 2026

use super::CellularAppearance;
use crate::materials::MaterialIdentifier;
use std::io;

/// An inactive 8x8 tile
pub struct TileData {
    /// The `MaterialIdentifier`s for cells in this `TileData`
    cell_material_identifiers: [[MaterialIdentifier; 8]; 8],
    /// The persistent per-cell appearance samples
    cell_appearances: [[CellularAppearance; 8]; 8],
    /// Persistent structural integrity samples
    cell_integrities: [[f32; 8]; 8],
    /// Persistent normalized material inventory
    cell_amounts: [[f32; 8]; 8],
    /// Persistent material temperature. `NaN` is only used for legacy dormant data.
    cell_temperatures: [[f32; 8]; 8],
}

impl TileData {
    /// The size of serialized `TileData` in bytes
    pub const SERIALIZED_SIZE: usize = 8 * 8 * 4 * 5;
    /// The pre-amount/temperature tile record size.
    pub const LEGACY_SERIALIZED_SIZE: usize = 8 * 8 * 4 * 3;

    /// The serialized size of one parallel cell field
    pub const CELL_FIELD_SERIALIZED_SIZE: usize = 8 * 8 * 4;

    /// An empty `TileInactive`
    pub const EMPTY: Self = Self {
        cell_material_identifiers: [[MaterialIdentifier::NULL; 8]; 8],
        cell_appearances: [[CellularAppearance::NEUTRAL; 8]; 8],
        cell_integrities: [[0.0; 8]; 8],
        cell_amounts: [[0.0; 8]; 8],
        cell_temperatures: [[0.0; 8]; 8],
    };

    /// Creates a tile filled with one material
    pub const fn new_filled(material_identifier: MaterialIdentifier) -> Self {
        Self {
            cell_material_identifiers: [[material_identifier; 8]; 8],
            cell_appearances: [[CellularAppearance::NEUTRAL; 8]; 8],
            cell_integrities: [[0.0; 8]; 8],
            cell_amounts: [[if material_identifier.as_u32() == 0 {
                0.0
            } else {
                1.0
            }; 8]; 8],
            cell_temperatures: [[if material_identifier.as_u32() == 0 {
                0.0
            } else {
                f32::NAN
            }; 8]; 8],
        }
    }

    /// Creates a tile filled with one material and one persistent appearance
    pub const fn new_filled_with_appearance(
        material_identifier: MaterialIdentifier,
        appearance: CellularAppearance,
    ) -> Self {
        Self {
            cell_material_identifiers: [[material_identifier; 8]; 8],
            cell_appearances: [[appearance; 8]; 8],
            cell_integrities: [[0.0; 8]; 8],
            cell_amounts: [[if material_identifier.as_u32() == 0 {
                0.0
            } else {
                1.0
            }; 8]; 8],
            cell_temperatures: [[if material_identifier.as_u32() == 0 {
                0.0
            } else {
                f32::NAN
            }; 8]; 8],
        }
    }

    /// Sets one local cell's material and persistent appearance
    pub fn set_cell(
        &mut self,
        x: usize,
        y: usize,
        material_identifier: MaterialIdentifier,
        appearance: CellularAppearance,
    ) {
        self.set_cell_with_integrity(x, y, material_identifier, appearance, 0.0);
    }

    /// Sets one local cell's material, appearance, and structural integrity
    pub fn set_cell_with_integrity(
        &mut self,
        x: usize,
        y: usize,
        material_identifier: MaterialIdentifier,
        appearance: CellularAppearance,
        integrity: f32,
    ) {
        self.cell_material_identifiers[y][x] = material_identifier;
        self.cell_appearances[y][x] = appearance;
        self.cell_integrities[y][x] = integrity;
        self.cell_amounts[y][x] = if material_identifier == MaterialIdentifier::NULL {
            0.0
        } else {
            1.0
        };
        self.cell_temperatures[y][x] = if material_identifier == MaterialIdentifier::NULL {
            0.0
        } else {
            f32::NAN
        };
    }

    /// Returns one local cell's material identifier
    pub(crate) const fn cell_material_identifier(&self, x: usize, y: usize) -> MaterialIdentifier {
        self.cell_material_identifiers[y][x]
    }

    /// Returns one local cell's persistent structural integrity.
    pub(crate) const fn cell_integrity(&self, x: usize, y: usize) -> f32 {
        self.cell_integrities[y][x]
    }

    /// Returns one local cell's persistent appearance
    pub(crate) const fn cell_appearance(&self, x: usize, y: usize) -> CellularAppearance {
        self.cell_appearances[y][x]
    }

    /// Returns one local cell's persistent material inventory.
    pub(crate) const fn cell_amount(&self, x: usize, y: usize) -> f32 {
        self.cell_amounts[y][x]
    }

    /// Returns one local cell's persistent temperature.
    pub(crate) const fn cell_temperature(&self, x: usize, y: usize) -> f32 {
        self.cell_temperatures[y][x]
    }

    /// Restores explicit state from GPU streaming or a modern chunk record.
    pub(crate) fn set_cell_state(&mut self, x: usize, y: usize, amount: f32, temperature: f32) {
        self.cell_amounts[y][x] = amount;
        self.cell_temperatures[y][x] = temperature;
    }

    /// Resolves dormant legacy or authored temperatures before active use or persistence.
    pub(crate) fn resolve_uninitialized_temperatures(
        &mut self,
        initial_temperature: impl Fn(MaterialIdentifier) -> f32,
    ) {
        for y in 0..8 {
            for x in 0..8 {
                let material = self.cell_material_identifiers[y][x];
                if material == MaterialIdentifier::NULL {
                    self.set_cell_state(x, y, 0.0, 0.0);
                } else if !self.cell_temperatures[y][x].is_finite() {
                    self.set_cell_state(x, y, 1.0, initial_temperature(material));
                }
            }
        }
    }

    /// Writes material identifiers in row-major GPU order
    pub fn serialize_material_identifiers<W: io::Write>(
        &self,
        writer: &mut W,
    ) -> Result<(), io::Error> {
        for row in &self.cell_material_identifiers {
            for material_identifier in row {
                writer.write_all(&material_identifier.as_u32().to_le_bytes())?;
            }
        }
        Ok(())
    }

    /// Writes appearances in the same row-major order as material identifiers
    pub fn serialize_appearances<W: io::Write>(&self, writer: &mut W) -> Result<(), io::Error> {
        for row in &self.cell_appearances {
            for appearance in row {
                writer.write_all(&appearance.0.to_le_bytes())?;
            }
        }
        Ok(())
    }

    /// Writes persistent integrity in row-major GPU order
    pub fn serialize_integrities<W: io::Write>(&self, writer: &mut W) -> Result<(), io::Error> {
        for row in &self.cell_integrities {
            for integrity in row {
                writer.write_all(&integrity.to_bits().to_le_bytes())?;
            }
        }
        Ok(())
    }

    /// Writes material inventories in row-major GPU order.
    pub fn serialize_amounts<W: io::Write>(&self, writer: &mut W) -> Result<(), io::Error> {
        for row in &self.cell_amounts {
            for amount in row {
                writer.write_all(&amount.to_bits().to_le_bytes())?;
            }
        }
        Ok(())
    }

    /// Writes material temperatures in row-major GPU order.
    pub fn serialize_temperatures<W: io::Write>(&self, writer: &mut W) -> Result<(), io::Error> {
        for row in &self.cell_temperatures {
            for temperature in row {
                writer.write_all(&temperature.to_bits().to_le_bytes())?;
            }
        }
        Ok(())
    }

    /// Deserializes binary data into a `TileData`
    pub fn deserialize<R: io::Read>(reader: &mut R) -> Result<TileData, io::Error> {
        let mut material_data: Vec<u8> = vec![0; Self::CELL_FIELD_SERIALIZED_SIZE];
        reader.read_exact(&mut material_data)?;
        let mut appearance_data: Vec<u8> = vec![0; Self::CELL_FIELD_SERIALIZED_SIZE];
        reader.read_exact(&mut appearance_data)?;
        let mut integrity_data: Vec<u8> = vec![0; Self::CELL_FIELD_SERIALIZED_SIZE];
        reader.read_exact(&mut integrity_data)?;
        let mut amount_data: Vec<u8> = vec![0; Self::CELL_FIELD_SERIALIZED_SIZE];
        reader.read_exact(&mut amount_data)?;
        let mut temperature_data: Vec<u8> = vec![0; Self::CELL_FIELD_SERIALIZED_SIZE];
        reader.read_exact(&mut temperature_data)?;
        let mut material_reader = material_data.as_slice();
        let mut appearance_reader = appearance_data.as_slice();
        let mut integrity_reader = integrity_data.as_slice();
        let mut amount_reader = amount_data.as_slice();
        let mut temperature_reader = temperature_data.as_slice();
        Self::deserialize_fields(
            &mut material_reader,
            &mut appearance_reader,
            &mut integrity_reader,
            &mut amount_reader,
            &mut temperature_reader,
        )
    }

    /// Deserializes the pre-amount/temperature layout, marking occupied temperatures unresolved.
    pub fn deserialize_legacy<R: io::Read>(reader: &mut R) -> Result<TileData, io::Error> {
        let mut material_data = vec![0; Self::CELL_FIELD_SERIALIZED_SIZE];
        reader.read_exact(&mut material_data)?;
        let mut appearance_data = vec![0; Self::CELL_FIELD_SERIALIZED_SIZE];
        reader.read_exact(&mut appearance_data)?;
        let mut integrity_data = vec![0; Self::CELL_FIELD_SERIALIZED_SIZE];
        reader.read_exact(&mut integrity_data)?;
        let mut material_reader = material_data.as_slice();
        let mut appearance_reader = appearance_data.as_slice();
        let mut integrity_reader = integrity_data.as_slice();
        let mut tile = Self::deserialize_legacy_fields(
            &mut material_reader,
            &mut appearance_reader,
            &mut integrity_reader,
        )?;
        for y in 0..8 {
            for x in 0..8 {
                if tile.cell_material_identifier(x, y) != MaterialIdentifier::NULL {
                    tile.set_cell_state(x, y, 1.0, f32::NAN);
                }
            }
        }
        Ok(tile)
    }

    /// Deserializes the two parallel cell fields from separate streams
    pub fn deserialize_fields<R: io::Read, A: io::Read, I: io::Read, M: io::Read, T: io::Read>(
        material_reader: &mut R,
        appearance_reader: &mut A,
        integrity_reader: &mut I,
        amount_reader: &mut M,
        temperature_reader: &mut T,
    ) -> Result<TileData, io::Error> {
        let mut tile_data: Self = Self::EMPTY;
        for row in &mut tile_data.cell_material_identifiers {
            for cell_material_identifier in row {
                let mut data: [u8; 4] = [0; 4];
                material_reader.read_exact(&mut data)?;
                *cell_material_identifier = MaterialIdentifier::from_u32(u32::from_le_bytes(data));
            }
        }
        for row in &mut tile_data.cell_appearances {
            for appearance in row {
                let mut data: [u8; 4] = [0; 4];
                appearance_reader.read_exact(&mut data)?;
                *appearance = CellularAppearance(u32::from_le_bytes(data));
            }
        }
        for row in &mut tile_data.cell_integrities {
            for integrity in row {
                let mut data: [u8; 4] = [0; 4];
                integrity_reader.read_exact(&mut data)?;
                *integrity = f32::from_bits(u32::from_le_bytes(data));
            }
        }
        for row in &mut tile_data.cell_amounts {
            for amount in row {
                let mut data = [0; 4];
                amount_reader.read_exact(&mut data)?;
                *amount = f32::from_bits(u32::from_le_bytes(data));
            }
        }
        for row in &mut tile_data.cell_temperatures {
            for temperature in row {
                let mut data = [0; 4];
                temperature_reader.read_exact(&mut data)?;
                *temperature = f32::from_bits(u32::from_le_bytes(data));
            }
        }
        tile_data.normalize_or_reject()?;
        Ok(tile_data)
    }

    /// Deserializes the three original parallel fields.
    pub fn deserialize_legacy_fields<R: io::Read, A: io::Read, I: io::Read>(
        material_reader: &mut R,
        appearance_reader: &mut A,
        integrity_reader: &mut I,
    ) -> Result<TileData, io::Error> {
        let mut tile_data = Self::EMPTY;
        for row in &mut tile_data.cell_material_identifiers {
            for identifier in row {
                let mut data = [0; 4];
                material_reader.read_exact(&mut data)?;
                *identifier = MaterialIdentifier::from_u32(u32::from_le_bytes(data));
            }
        }
        for row in &mut tile_data.cell_appearances {
            for appearance in row {
                let mut data = [0; 4];
                appearance_reader.read_exact(&mut data)?;
                *appearance = CellularAppearance(u32::from_le_bytes(data));
            }
        }
        for row in &mut tile_data.cell_integrities {
            for integrity in row {
                let mut data = [0; 4];
                integrity_reader.read_exact(&mut data)?;
                *integrity = f32::from_bits(u32::from_le_bytes(data));
            }
        }
        Ok(tile_data)
    }

    fn normalize_or_reject(&mut self) -> Result<(), io::Error> {
        for y in 0..8 {
            for x in 0..8 {
                if self.cell_material_identifiers[y][x] == MaterialIdentifier::NULL {
                    self.cell_amounts[y][x] = 0.0;
                    self.cell_temperatures[y][x] = 0.0;
                } else if !self.cell_amounts[y][x].is_finite()
                    || self.cell_amounts[y][x] <= 0.0
                    || !self.cell_temperatures[y][x].is_finite()
                    || self.cell_temperatures[y][x] < 0.0
                {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "invalid cellular amount or temperature",
                    ));
                }
            }
        }
        Ok(())
    }

    /// Serializes a `TileData` into binary data
    pub fn serialize<W: io::Write>(&self, writer: &mut W) -> Result<(), io::Error> {
        self.validate_for_serialization()?;
        self.serialize_material_identifiers(writer)?;
        self.serialize_appearances(writer)?;
        self.serialize_integrities(writer)?;
        self.serialize_amounts(writer)?;
        self.serialize_temperatures(writer)
    }

    fn validate_for_serialization(&self) -> Result<(), io::Error> {
        for y in 0..8 {
            for x in 0..8 {
                if self.cell_material_identifiers[y][x] != MaterialIdentifier::NULL
                    && (!self.cell_amounts[y][x].is_finite()
                        || self.cell_amounts[y][x] <= 0.0
                        || !self.cell_temperatures[y][x].is_finite()
                        || self.cell_temperatures[y][x] < 0.0)
                {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "invalid cellular amount or temperature",
                    ));
                }
            }
        }
        Ok(())
    }
}
