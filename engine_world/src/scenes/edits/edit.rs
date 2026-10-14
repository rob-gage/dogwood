// Copyright Rob Gage 2026

use crate::{
    materials::MaterialIdentifier,
    scenes::{
        cellular_tiles::{
            CellCoordinates,
            CellularTileCoordinates,
            CELLULAR_TILE_SIZE,
        },
        ScenePosition,
    },
};
use super::{
    EditError,
    EditMode,
    EDIT_CELLULAR_TILE_COUNT_MAXIMUM,
};
use std::collections::BTreeMap;

/// Cells of a scene to fill with materials, grouped by cellular tile
#[derive(Clone, Debug)]
pub struct Edit {
    /// Whether filled cells keep or replace existing matter
    pub mode: EditMode,
    /// Filled cells as bit `y * 8 + x`, material identifiers, and packed RGBA colors of each tile
    pub cellular_tiles: BTreeMap<
        CellularTileCoordinates,
        (u64, [MaterialIdentifier; 64], [u32; 64]),
    >,
}

impl Edit {

    /// Creates an edit without cells
    pub fn new(mode: EditMode) -> Self {
        Self { mode, cellular_tiles: BTreeMap::new() }
    }

    /// Fills one cell with a material and packed RGBA color, replacing an earlier fill of the cell
    pub fn fill_cell(
        &mut self,
        cell: CellCoordinates,
        material_identifier: MaterialIdentifier,
        color: u32,
    ) {
        let cell_index: usize = (cell.y.rem_euclid(CELLULAR_TILE_SIZE) * CELLULAR_TILE_SIZE
            + cell.x.rem_euclid(CELLULAR_TILE_SIZE)) as usize;
        let (cells, material_identifiers, colors): &mut (u64, [MaterialIdentifier; 64], [u32; 64]) =
            self.cellular_tiles
                .entry(cell.cellular_tile_coordinates())
                .or_insert((0, [MaterialIdentifier::EMPTY; 64], [0; 64]));
        *cells |= 1 << cell_index;
        material_identifiers[cell_index] = material_identifier;
        colors[cell_index] = color;
    }

    /// Fills every cell whose center lies within a radius in tiles of a segment
    pub fn fill_segment(
        &mut self,
        start: ScenePosition,
        end: ScenePosition,
        radius: f64,
        material_identifier: MaterialIdentifier,
        color: u32,
    ) -> Result<(), EditError> {
        let (relative_x, relative_y): (f64, f64) = end.relative_position(start);
        if !radius.is_finite() || radius < 0.0 || !relative_x.is_finite() || !relative_y.is_finite()
        { return Ok(()); }
        // rasterize in cells relative to the start tile so distant positions stay exact
        let cellular_tile_size: f64 = CELLULAR_TILE_SIZE as f64;
        let start_x: f64 =
            f64::from(start.offset_x) / ScenePosition::OFFSET_UNITS_PER_TILE * cellular_tile_size;
        let start_y: f64 =
            f64::from(start.offset_y) / ScenePosition::OFFSET_UNITS_PER_TILE * cellular_tile_size;
        let segment_x: f64 = relative_x * cellular_tile_size;
        let segment_y: f64 = relative_y * cellular_tile_size;
        let radius: f64 = radius * cellular_tile_size;
        let minimum_x: i64 = (start_x + segment_x.min(0.0) - radius).floor() as i64;
        let maximum_x: i64 = (start_x + segment_x.max(0.0) + radius).floor() as i64;
        let minimum_y: i64 = (start_y + segment_y.min(0.0) - radius).floor() as i64;
        let maximum_y: i64 = (start_y + segment_y.max(0.0) + radius).floor() as i64;
        // reject the segment before rasterizing when its bounds span too many tiles
        let cellular_tile_count: i128 = (i128::from(maximum_x.div_euclid(CELLULAR_TILE_SIZE))
            - i128::from(minimum_x.div_euclid(CELLULAR_TILE_SIZE)) + 1)
            * (i128::from(maximum_y.div_euclid(CELLULAR_TILE_SIZE))
                - i128::from(minimum_y.div_euclid(CELLULAR_TILE_SIZE)) + 1);
        if cellular_tile_count > EDIT_CELLULAR_TILE_COUNT_MAXIMUM as i128 {
            return Err(EditError::CapacityReached);
        }
        // fill every cell whose center is within the radius of its closest segment point
        let segment_length_squared: f64 = segment_x * segment_x + segment_y * segment_y;
        for cell_y in minimum_y..=maximum_y {
            for cell_x in minimum_x..=maximum_x {
                let center_x: f64 = cell_x as f64 + 0.5 - start_x;
                let center_y: f64 = cell_y as f64 + 0.5 - start_y;
                let projection: f64 = if segment_length_squared == 0.0 { 0.0 } else {
                    ((center_x * segment_x + center_y * segment_y) / segment_length_squared)
                        .clamp(0.0, 1.0)
                };
                let distance_x: f64 = center_x - projection * segment_x;
                let distance_y: f64 = center_y - projection * segment_y;
                if distance_x * distance_x + distance_y * distance_y > radius * radius { continue; }
                self.fill_cell(
                    CellCoordinates {
                        x: start.tile_x * CELLULAR_TILE_SIZE + cell_x,
                        y: start.tile_y * CELLULAR_TILE_SIZE + cell_y,
                    },
                    material_identifier,
                    color,
                );
            }
        }
        Ok(())
    }

}
