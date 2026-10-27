// Copyright Rob Gage 2026

use engine::{
    materials::MaterialIdentifier,
    scenes::{
        cellular_tiles::{
            CellCoordinates,
            CELLULAR_TILE_SIZE,
        },
        edits::{
            Edit,
            EditError,
        },
        ScenePosition,
    },
};

/// The shape stamped along a stroke between two cells
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum EditorBrush {
    /// Fills cells within half the brush size of the stroke
    Circle,
    /// Fills a square of the brush size around every cell of the stroke
    Square,
}

impl EditorBrush {

    /// Fills the cells covered by a stroke between two cells with a brush size in cells
    pub fn fill_stroke(
        self,
        edit: &mut Edit,
        start: CellCoordinates,
        end: CellCoordinates,
        size: u32,
        material_identifier: MaterialIdentifier,
        color: u32,
    ) -> Result<(), EditError> {
        match self {
            // a segment between cell centers covers exactly the cells a circle passes over
            EditorBrush::Circle => {
                let cell_center = |cell: CellCoordinates| -> ScenePosition {
                    let offset = |coordinate: i64| -> u32 {
                        ((coordinate.rem_euclid(CELLULAR_TILE_SIZE) as f64 + 0.5)
                            / CELLULAR_TILE_SIZE as f64
                            * ScenePosition::OFFSET_UNITS_PER_TILE) as u32
                    };
                    ScenePosition {
                        tile_x: cell.x.div_euclid(CELLULAR_TILE_SIZE),
                        tile_y: cell.y.div_euclid(CELLULAR_TILE_SIZE),
                        offset_x: offset(cell.x),
                        offset_y: offset(cell.y),
                    }
                };
                edit.fill_segment(
                    cell_center(start),
                    cell_center(end),
                    f64::from(size) / 2.0 / CELLULAR_TILE_SIZE as f64,
                    material_identifier,
                    color,
                    None,
                )
            }
            // stamp a square at every cell along the stroke
            EditorBrush::Square => {
                let step_count: i64 = (end.x - start.x).abs().max((end.y - start.y).abs());
                let low: i64 = -(i64::from(size) - 1) / 2;
                let high: i64 = i64::from(size) / 2;
                for step in 0..=step_count {
                    let (x, y): (i64, i64) = if step_count == 0 { (start.x, start.y) } else {
                        (
                            start.x + ((end.x - start.x) * step * 2 + step_count)
                                .div_euclid(step_count * 2),
                            start.y + ((end.y - start.y) * step * 2 + step_count)
                                .div_euclid(step_count * 2),
                        )
                    };
                    for offset_y in low..=high {
                        for offset_x in low..=high {
                            edit.fill_cell(
                                CellCoordinates { x: x + offset_x, y: y + offset_y },
                                material_identifier,
                                color,
                                None,
                            );
                        }
                    }
                }
                Ok(())
            }
        }
    }

}
