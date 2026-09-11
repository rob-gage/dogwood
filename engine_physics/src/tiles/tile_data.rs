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
}

impl TileData {

    /// The size of serialized `TileData` in bytes
    pub const SERIALIZED_SIZE: usize = 8 * 8 * 4 * 2;

    /// The serialized size of one parallel cell field
    pub const CELL_FIELD_SERIALIZED_SIZE: usize = 8 * 8 * 4;

    /// An empty `TileInactive`
    pub const EMPTY: Self = Self {
        cell_material_identifiers: [[MaterialIdentifier::NULL; 8]; 8],
        cell_appearances: [[CellularAppearance::NEUTRAL; 8]; 8],
    };

    /// Creates a tile filled with one material
    pub const fn new_filled(material_identifier: MaterialIdentifier) -> Self {
        Self {
            cell_material_identifiers: [[material_identifier; 8]; 8],
            cell_appearances: [[CellularAppearance::NEUTRAL; 8]; 8],
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
        self.cell_material_identifiers[y][x] = material_identifier;
        self.cell_appearances[y][x] = appearance;
    }

    /// Returns one local cell's material identifier
    #[cfg(test)]
    pub const fn cell_material_identifier(&self, x: usize, y: usize) -> MaterialIdentifier {
        self.cell_material_identifiers[y][x]
    }

    /// Returns one local cell's persistent appearance
    #[cfg(test)]
    pub const fn cell_appearance(&self, x: usize, y: usize) -> CellularAppearance {
        self.cell_appearances[y][x]
    }

    /// Writes material identifiers in row-major GPU order
    pub fn serialize_material_identifiers<W: io::Write>(&self, writer: &mut W) -> Result<(), io::Error> {
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

    /// Deserializes binary data into a `TileData`
    pub fn deserialize<R: io::Read>(reader: &mut R) -> Result<TileData, io::Error> {
        let mut material_data: Vec<u8> = vec![0; Self::CELL_FIELD_SERIALIZED_SIZE];
        reader.read_exact(&mut material_data)?;
        let mut appearance_data: Vec<u8> = vec![0; Self::CELL_FIELD_SERIALIZED_SIZE];
        reader.read_exact(&mut appearance_data)?;
        let mut material_reader = material_data.as_slice();
        let mut appearance_reader = appearance_data.as_slice();
        Self::deserialize_fields(&mut material_reader, &mut appearance_reader)
    }

    /// Deserializes the two parallel cell fields from separate streams
    pub fn deserialize_fields<R: io::Read, A: io::Read>(
        material_reader: &mut R,
        appearance_reader: &mut A,
    ) -> Result<TileData, io::Error> {
        let mut tile_data: Self = Self::EMPTY;
        for row in &mut tile_data.cell_material_identifiers {
            for cell_material_identifier in row {
                let mut data: [u8; 4] = [0; 4];
                material_reader.read_exact(&mut data)?;
                *cell_material_identifier = MaterialIdentifier::from_u32(
                    u32::from_le_bytes(data)
                );
            }
        }
        for row in &mut tile_data.cell_appearances {
            for appearance in row {
                let mut data: [u8; 4] = [0; 4];
                appearance_reader.read_exact(&mut data)?;
                *appearance = CellularAppearance(u32::from_le_bytes(data));
            }
        }
        Ok(tile_data)
    }

    /// Serializes a `TileData` into binary data
    pub fn serialize<W: io::Write>(&self, writer: &mut W) -> Result<(), io::Error> {
        self.serialize_material_identifiers(writer)?;
        self.serialize_appearances(writer)
    }

}
