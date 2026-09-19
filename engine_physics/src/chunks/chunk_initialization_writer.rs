// Copyright Rob Gage 2026

use crate::materials::MaterialIdentifier;
use crate::tiles::CellCoordinates;
use crate::tiles::CellularAppearance;

use super::Chunk;

/// Direct bulk writer for the initial contents of a newly generated `Chunk`.
pub struct ChunkInitializationWriter<'a> {
    chunk: &'a mut Chunk,
    initialized_cell_count: usize,
}

impl<'a> ChunkInitializationWriter<'a> {
    pub(crate) fn new(chunk: &'a mut Chunk) -> Self {
        Self {
            chunk,
            initialized_cell_count: 0,
        }
    }

    /// Writes one absolute world cell without creating a runtime edit.
    pub fn set_cell(
        &mut self,
        coordinates: CellCoordinates,
        material_identifier: MaterialIdentifier,
        appearance: CellularAppearance,
    ) {
        self.set_cell_with_integrity(coordinates, material_identifier, appearance, 0.0);
    }

    /// Writes one absolute world cell with persistent integrity.
    pub fn set_cell_with_integrity(
        &mut self,
        coordinates: CellCoordinates,
        material_identifier: MaterialIdentifier,
        appearance: CellularAppearance,
        integrity: f32,
    ) {
        let local_coordinates: [usize; 2] = coordinates.local_tile_coordinates();
        self.chunk
            .set_cell_with_integrity(
                coordinates.tile_coordinates(),
                local_coordinates[0],
                local_coordinates[1],
                material_identifier,
                appearance,
                integrity,
            )
            .unwrap();
        self.initialized_cell_count += 1;
    }

    /// Fills a contiguous absolute cell rectangle without creating runtime edits.
    pub fn fill_cells(
        &mut self,
        origin: CellCoordinates,
        width: u16,
        height: u16,
        material_identifier: MaterialIdentifier,
        appearance: CellularAppearance,
    ) {
        for y in origin.y..origin.y + i32::from(height) {
            for x in origin.x..origin.x + i32::from(width) {
                self.set_cell(CellCoordinates { x, y }, material_identifier, appearance);
            }
        }
    }

    /// Returns the number of cells written through this writer.
    pub const fn initialized_cell_count(&self) -> usize {
        self.initialized_cell_count
    }
}
