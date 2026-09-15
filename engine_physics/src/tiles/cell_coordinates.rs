// Copyright Rob Gage 2026

use super::TileCoordinates;

/// The position of a cell within a `Scene`
///
/// Positions move higher up as Y increases, and they move further right as X increases.
#[derive(Copy, Clone, Eq, PartialEq, Hash)]
pub struct CellCoordinates {
    pub x: i32,
    pub y: i32,
}

impl CellCoordinates {
    /// Returns the cell containing a continuous world position in tile units
    pub fn from_world_position(position: [f32; 2]) -> Self {
        Self {
            x: (position[0] * 8.0).floor() as i32,
            y: (position[1] * 8.0).floor() as i32,
        }
    }

    /// Returns the tile containing these cell coordinates
    pub const fn tile_coordinates(self) -> TileCoordinates {
        TileCoordinates {
            x: self.x.div_euclid(8),
            y: self.y.div_euclid(8),
        }
    }

    /// Returns these cell coordinates relative to their containing tile
    pub const fn local_tile_coordinates(self) -> [usize; 2] {
        [self.x.rem_euclid(8) as usize, self.y.rem_euclid(8) as usize]
    }

    /// Returns the deterministic persistent-appearance seed for these coordinates
    pub const fn appearance_seed(self) -> u32 {
        (self.x as u32).wrapping_mul(0x9e37_79b9) ^ (self.y as u32).wrapping_mul(0x85eb_ca6b)
    }
}

#[cfg(test)]
mod tests {
    use super::CellCoordinates;

    #[test]
    fn from_world_position_floors_cell_coordinates() {
        assert!(
            CellCoordinates::from_world_position([0.01, 0.0]) == CellCoordinates { x: 0, y: 0 }
        );
        assert!(
            CellCoordinates::from_world_position([0.99, 0.0]) == CellCoordinates { x: 7, y: 0 }
        );
        assert!(CellCoordinates::from_world_position([1.0, 0.0]) == CellCoordinates { x: 8, y: 0 });
        assert!(
            CellCoordinates::from_world_position([-0.01, 0.0]) == CellCoordinates { x: -1, y: 0 }
        );
        assert!(
            CellCoordinates::from_world_position([-1.0, -0.01]) == CellCoordinates { x: -8, y: -1 }
        );
    }
}
