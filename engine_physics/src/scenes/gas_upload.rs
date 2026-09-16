// Copyright Rob Gage 2026

use crate::{
    chunks::ChunkGasCell,
    materials::{Material, MaterialRegistry},
    tiles::TileArea,
};
use std::io;

/// Owns dormant gas records while they are restored to GPU residency
pub struct GasUpload {
    /// The incoming world-tile area represented by this transfer
    pub area: TileArea,
    /// Persistent records removed from their chunks for restoration
    pub cells: Vec<ChunkGasCell>,
}

impl GasUpload {
    /// Creates a synchronous fixed-capacity gas upload
    pub const fn new(area: TileArea, cells: Vec<ChunkGasCell>) -> Self {
        Self { area, cells }
    }

    /// Validates incoming records against their area and material registry
    pub fn validate(&self, materials: &MaterialRegistry) -> Result<(), io::Error> {
        for cell in &self.cells {
            if !self.area.contains(cell.tile_coordinates()) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Dormant gas cell is outside its upload area",
                ));
            }
            cell.validate()?;
            if !cell.species.iter().all(|(identifier, _)| {
                matches!(materials.get(*identifier), Some(Material::Gas { .. }))
            }) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Dormant cell references an unregistered gas material",
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::{
        materials::MaterialIdentifier,
        tiles::{CellCoordinates, TileCoordinates},
    };

    #[test]
    fn rejects_a_cell_outside_its_upload_area() {
        let upload: GasUpload = GasUpload::new(
            TileArea::new(TileCoordinates { x: 0, y: 0 }, 1, 1),
            vec![ChunkGasCell {
                coordinates: CellCoordinates { x: 8, y: 0 },
                velocity: [0.0; 2],
                species: vec![(MaterialIdentifier::from_u32(1), 1.0)],
                temperature: 293.15,
            }],
        );
        assert!(upload.validate(&MaterialRegistry::new()).is_err());
    }
}
