// Copyright Rob Gage 2026

use engine::physics::tiles::CellCoordinates;

/// A discrete cellular brush used by the editor
#[derive(Copy, Clone, Eq, PartialEq)]
pub enum EditorBrush {
    /// A square cellular brush
    Square { size: u16 },
    /// A circle cellular brush
    Circle { size: u16 },
}

impl EditorBrush {
    /// The maximum brush size used by this editor implementation
    const SIZE_MAXIMUM: u16 = 64;

    /// Creates a one-cell square brush
    pub const fn new() -> Self {
        Self::Square { size: 1 }
    }

    /// Returns whether this brush is square
    pub const fn is_square(self) -> bool {
        matches!(self, Self::Square { .. })
    }

    /// Returns the brush size in cells
    pub const fn size(self) -> u16 {
        match self {
            Self::Square { size } | Self::Circle { size } => size,
        }
    }

    /// Selects the square shape while preserving the current size
    pub fn select_square(&mut self) -> bool {
        if self.is_square() {
            return false;
        }
        *self = Self::Square { size: self.size() };
        true
    }

    /// Selects the circle shape while preserving the current size
    pub fn select_circle(&mut self) -> bool {
        if !self.is_square() {
            return false;
        }
        *self = Self::Circle { size: self.size() };
        true
    }

    /// Adjusts the brush size within the supported range
    pub fn adjust_size(&mut self, adjustment: i32) -> bool {
        let size: u16 =
            (i32::from(self.size()) + adjustment).clamp(1, i32::from(Self::SIZE_MAXIMUM)) as u16;
        if size == self.size() {
            return false;
        }
        match self {
            Self::Square { size: current_size } | Self::Circle { size: current_size } => {
                *current_size = size
            }
        }
        true
    }

    /// Returns the exact cells affected when this brush is anchored at a cell
    pub fn cells(self, anchor: CellCoordinates) -> Vec<CellCoordinates> {
        let size: i64 = i64::from(self.size());
        let start_x: i64 = i64::from(anchor.x) - (size - 1) / 2;
        let start_y: i64 = i64::from(anchor.y) - (size - 1) / 2;
        let mut cells: Vec<CellCoordinates> = Vec::with_capacity((size * size) as usize);
        for y in start_y..start_y + size {
            for x in start_x..start_x + size {
                if x < i64::from(i32::MIN)
                    || x > i64::from(i32::MAX)
                    || y < i64::from(i32::MIN)
                    || y > i64::from(i32::MAX)
                {
                    continue;
                }
                if matches!(self, Self::Circle { .. }) {
                    let center2_x: i128 = 2 * i128::from(start_x) + i128::from(size);
                    let center2_y: i128 = 2 * i128::from(start_y) + i128::from(size);
                    let dx: i128 = 2 * i128::from(x) + 1 - center2_x;
                    let dy: i128 = 2 * i128::from(y) + 1 - center2_y;
                    if dx * dx + dy * dy > i128::from(size * size) {
                        continue;
                    }
                }
                cells.push(CellCoordinates {
                    x: x as i32,
                    y: y as i32,
                });
            }
        }
        cells
    }

    /// Returns every anchor along the inclusive integer stroke segment
    pub fn stroke_anchors(start: CellCoordinates, end: CellCoordinates) -> Vec<CellCoordinates> {
        let mut x: i64 = i64::from(start.x);
        let mut y: i64 = i64::from(start.y);
        let end_x: i64 = i64::from(end.x);
        let end_y: i64 = i64::from(end.y);
        let delta_x: i64 = (end_x - x).abs();
        let step_x: i64 = if x < end_x { 1 } else { -1 };
        let delta_y: i64 = -(end_y - y).abs();
        let step_y: i64 = if y < end_y { 1 } else { -1 };
        let mut error: i64 = delta_x + delta_y;
        let mut anchors: Vec<CellCoordinates> = Vec::new();
        loop {
            anchors.push(CellCoordinates {
                x: x as i32,
                y: y as i32,
            });
            if x == end_x && y == end_y {
                break;
            }
            let error2: i64 = 2 * error;
            if error2 >= delta_y {
                error += delta_y;
                x += step_x;
            }
            if error2 <= delta_x {
                error += delta_x;
                y += step_y;
            }
        }
        anchors
    }
}
