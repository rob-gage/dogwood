// Copyright Rob Gage 2026

use std::io;

use crate::chunks::ChunkGasCell;
use crate::materials::Material;
use crate::materials::MaterialRegistry;
use crate::tiles::TileArea;

/// Owns dormant gas records while they are restored to Accelerator residency
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
