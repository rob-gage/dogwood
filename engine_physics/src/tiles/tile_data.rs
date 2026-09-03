// Copyright Rob Gage 2026

use crate::materials::MaterialIdentifier;
use std::io;

/// An inactive 8x8 tile
pub struct TileData {
    /// The `MaterialIdentifier`s for cells in this `TileData`
    cell_material_identifiers: [[MaterialIdentifier; 8]; 8]
}

impl TileData {

    /// An empty `TileInactive`
    pub const EMPTY: Self = Self {
        cell_material_identifiers: [[MaterialIdentifier::NULL; 8]; 8],
    };

    /// Deserializes binary data into a `TileData`
    pub fn deserialize<R: io::Read>(reader: &mut R) -> Result<TileData, io::Error> {
        let mut cell_material_identifiers: [[MaterialIdentifier; 8]; 8] =
            [[MaterialIdentifier::NULL; 8]; 8];
        for row in &mut cell_material_identifiers {
            for cell_material_identifier in row {
                let mut data: [u8; 4] = [0; 4];
                reader.read_exact(&mut data)?;
                *cell_material_identifier = MaterialIdentifier::from_u32(
                    u32::from_le_bytes(data)
                );
            }
        }
        Ok(Self { cell_material_identifiers })
    }

    /// Serializes a `TileData` into binary data
    pub fn serialize<W: io::Write>(&self, writer: &mut W) -> Result<(), io::Error> {
        for row in &self.cell_material_identifiers {
            for cell_material_identifier in row {
                writer.write_all(&cell_material_identifier.as_u32().to_le_bytes())?;
            }
        }
        Ok(())
    }

}
