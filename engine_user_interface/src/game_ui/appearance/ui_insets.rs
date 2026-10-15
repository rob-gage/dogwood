// Copyright Rob Gage 2026

/// Four-sided distances used by nine-slice borders
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct UiInsets {
    /// Distance on the top side
    pub top: f32,
    /// Distance on the right side
    pub right: f32,
    /// Distance on the bottom side
    pub bottom: f32,
    /// Distance on the left side
    pub left: f32,
}

impl UiInsets {

    /// Creates four-sided distances in top-right-bottom-left order
    pub const fn new(top: f32, right: f32, bottom: f32, left: f32) -> Self {
        Self {
            top,
            right,
            bottom,
            left,
        }
    }

    /// Creates equal distances on all sides
    pub const fn same(distance: f32) -> Self {
        Self::new(distance, distance, distance, distance)
    }

    /// Returns the horizontal total
    pub const fn horizontal(self) -> f32 {
        self.left + self.right
    }

    /// Returns the vertical total
    pub const fn vertical(self) -> f32 {
        self.top + self.bottom
    }

    /// Multiplies every side by a scale
    pub const fn scaled(self, scale: f32) -> Self {
        Self::new(
            self.top * scale,
            self.right * scale,
            self.bottom * scale,
            self.left * scale,
        )
    }
}
