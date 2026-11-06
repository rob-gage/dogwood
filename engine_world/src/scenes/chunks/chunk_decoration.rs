// Copyright Rob Gage 2026

use crate::materials::MaterialIdentifier;

/// One piece of a composed plant: a frame of grayscale pixel art placed around a root cell, colored
/// through a ramp in the atlas, carried rigidly by the motion field the farther it lies from its
/// root, and vanishing once the support cell of its root changes. Every art pixel is one world cell
/// across, at whatever position and rotation the piece has
///
/// A plant is any number of pieces sharing a root cell, support, and reach.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct ChunkDecoration {
    /// Root cell relative to the chunk corner
    pub x: u16,
    /// Root cell relative to the chunk corner
    pub y: u16,
    /// Atlas pixel at the frame's left edge
    pub atlas_x: u16,
    /// Atlas row at the frame's top edge, counted from the top of the atlas
    pub atlas_y: u16,
    /// Frame width in pixels
    pub width: u8,
    /// Frame height in pixels
    pub height: u8,
    /// Pivot x in art pixels from bottom left
    pub pivot_x: u8,
    /// Pivot y in art pixels from bottom left
    pub pivot_y: u8,
    /// Pivot's horizontal offset from the root in half cells
    pub offset_x: i8,
    /// Pivot's vertical offset from the root in half cells
    pub offset_y: i8,
    /// Counterclockwise turn around the pivot in 1/256ths of a full turn
    pub rotation: u8,
    /// Cells drawn up from the frame's bottom, repeating its rows past its height, or zero for
    /// exactly its height
    pub length: u8,
    /// Distance in cells from the root to the plant's farthest point, where bending is fullest
    pub reach: u8,
    /// How far the plant bends, where zero is rigid
    pub flex: u8,
    /// Stable phase of its motion
    pub phase: u8,
    /// Atlas row of the ramp coloring the art; art whose green channel is set uses the next row
    pub ramp_row: u16,
    /// Light emitted by the art's glowing pixels, whose alpha is below one
    pub glow: u8,
    /// Brightness multiplier, where 255 leaves the ramp's colors as they are
    pub shade: u8,
    /// Support cell offset from the root cell
    pub support_x: i8,
    /// Support cell offset from the root cell
    pub support_y: i8,
    /// How far this piece flutters around its own pivot, where zero is not at all
    pub flutter: u8,
    /// Whether the art is mirrored
    pub is_mirrored: bool,
    /// Material required at the support cell
    pub support_material: MaterialIdentifier,
    /// Whether this piece is in front of cellular content
    pub is_front: bool,
}

impl ChunkDecoration {

    /// Bytes of one instance
    pub const BYTE_COUNT: usize = 32;

    /// Returns this piece's instance in the layout of the decoration renderer
    pub fn instance_bytes(&self) -> [u8; Self::BYTE_COUNT] {
        let words: [u32; 8] = [
            u32::from(self.x) | u32::from(self.y) << 16,
            u32::from(self.atlas_x) | u32::from(self.atlas_y) << 16,
            u32::from(self.width) | u32::from(self.height) << 8
                | u32::from(self.pivot_x) << 16 | u32::from(self.pivot_y) << 24,
            u32::from(self.offset_x as u8) | u32::from(self.offset_y as u8) << 8
                | u32::from(self.rotation) << 16 | u32::from(self.length) << 24,
            u32::from(self.reach) << 8
                | u32::from(self.flex) << 16 | u32::from(self.phase) << 24,
            u32::from(self.ramp_row) | u32::from(self.glow) << 16 | u32::from(self.shade) << 24,
            u32::from(self.support_x as u8) | u32::from(self.support_y as u8) << 8
                | u32::from(self.flutter) << 16 | u32::from(self.is_mirrored) << 24,
            self.support_material.as_u32(),
        ];
        let mut bytes: [u8; Self::BYTE_COUNT] = [0; Self::BYTE_COUNT];
        for (index, word) in words.iter().enumerate() {
            bytes[index * 4..index * 4 + 4].copy_from_slice(&word.to_le_bytes());
        }
        bytes
    }

}
