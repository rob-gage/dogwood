// Copyright Rob Gage 2026

use crate::materials::MaterialIdentifier;
use crate::tiles::CellCoordinates;
use crate::tiles::CellularAppearance;
use crate::tiles::TileCoordinates;
use crate::tiles::TileData;

use super::Chunk;

/// Direct bulk writer for the initial contents of a newly generated `Chunk`.
pub struct ChunkInitializationWriter<'a> {
    chunk: &'a mut Chunk,
    initialized_cell_count: usize,
    initialized_tile_count: usize,
    absolute_cell_write_count: usize,
}

impl<'a> ChunkInitializationWriter<'a> {
    pub(crate) fn new(chunk: &'a mut Chunk) -> Self {
        Self {
            chunk,
            initialized_cell_count: 0,
            initialized_tile_count: 0,
            absolute_cell_write_count: 0,
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
        self.absolute_cell_write_count += 1;
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

    /// Installs one complete tile without routing through absolute cell writes.
    pub fn set_tile(&mut self, coordinates: TileCoordinates, data: TileData) -> Result<(), ()> {
        self.chunk.set_tile(coordinates, data)?;
        self.initialized_tile_count += 1;
        self.initialized_cell_count += 64;
        Ok(())
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
        self.fill_cells_with_integrity(origin, width, height, material_identifier, appearance, 0.0);
    }

    /// Fills a rectangle through tile-native writes, using local cell writes only at boundaries.
    pub fn fill_cells_with_integrity(
        &mut self,
        origin: CellCoordinates,
        width: u16,
        height: u16,
        material_identifier: MaterialIdentifier,
        appearance: CellularAppearance,
        integrity: f32,
    ) {
        if width == 0 || height == 0 {
            return;
        }
        let maximum = CellCoordinates {
            x: origin.x + i32::from(width) - 1,
            y: origin.y + i32::from(height) - 1,
        };
        for tile_y in origin.y.div_euclid(8)..=maximum.y.div_euclid(8) {
            for tile_x in origin.x.div_euclid(8)..=maximum.x.div_euclid(8) {
                let minimum_local_x = if tile_x == origin.x.div_euclid(8) {
                    origin.x.rem_euclid(8) as usize
                } else {
                    0
                };
                let maximum_local_x = if tile_x == maximum.x.div_euclid(8) {
                    maximum.x.rem_euclid(8) as usize
                } else {
                    7
                };
                let minimum_local_y = if tile_y == origin.y.div_euclid(8) {
                    origin.y.rem_euclid(8) as usize
                } else {
                    0
                };
                let maximum_local_y = if tile_y == maximum.y.div_euclid(8) {
                    maximum.y.rem_euclid(8) as usize
                } else {
                    7
                };
                let tile_coordinates = TileCoordinates {
                    x: tile_x,
                    y: tile_y,
                };
                if minimum_local_x == 0
                    && maximum_local_x == 7
                    && minimum_local_y == 0
                    && maximum_local_y == 7
                {
                    self.set_tile(
                        tile_coordinates,
                        TileData::new_filled_with_appearance_and_integrity(
                            material_identifier,
                            appearance,
                            integrity,
                        ),
                    )
                    .unwrap();
                    continue;
                }
                let mut tile = TileData::EMPTY;
                for local_y in minimum_local_y..=maximum_local_y {
                    for local_x in minimum_local_x..=maximum_local_x {
                        tile.set_cell_with_integrity(
                            local_x,
                            local_y,
                            material_identifier,
                            appearance,
                            integrity,
                        );
                    }
                }
                self.set_tile(tile_coordinates, tile).unwrap();
            }
        }
    }

    /// Returns the number of cells written through this writer.
    pub const fn initialized_cell_count(&self) -> usize {
        self.initialized_cell_count
    }

    /// Returns the number of tile installations performed through this writer.
    pub const fn initialized_tile_count(&self) -> usize {
        self.initialized_tile_count
    }

    /// Returns the number of calls made through the absolute cell-write API.
    pub const fn absolute_cell_write_count(&self) -> usize {
        self.absolute_cell_write_count
    }
}
