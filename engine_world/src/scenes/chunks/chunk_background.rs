// Copyright Rob Gage 2026

/// Image and sprites generated behind one chunk, never saved
#[derive(Clone, Debug, Default)]
pub struct ChunkBackground {
    /// Packed little-endian straight-alpha sRGB RGBA8 color of each cell of the chunk, in rows from
    /// the bottom
    pub image: Option<Vec<u32>>,
    /// Sprites from the scene's background sprite sheet
    pub sprites: Vec<ChunkBackgroundSprite>,
}

/// One sprite of a chunk's background
#[derive(Clone, Debug, PartialEq)]
pub struct ChunkBackgroundSprite {
    /// Horizontal center relative to the chunk's corner in cells
    pub x: f32,
    /// Vertical center relative to the chunk's corner in cells
    pub y: f32,
    /// Width in cells, mirrored when negative
    pub width: f32,
    /// Height in cells, mirrored when negative
    pub height: f32,
    /// Counterclockwise rotation in radians
    pub rotation: f32,
    /// Frame of the sprite sheet
    pub frame: u32,
    /// Part of the frame drawn, as fractions from its top left: left, top, right, and bottom; keeping
    /// this tight around what a frame shows spares the GPU its empty pixels
    pub crop: [f32; 4],
    /// Multiplier of the frame's linear color and alpha
    pub color: [f32; 4],
}

/// A chunk's background prepared on a streaming thread, ready for upload without further work
#[derive(Clone, Debug, Default)]
pub struct ChunkBackgroundUpload {
    /// Bytes of the chunk's image, in rows from the top as textures expect
    pub image: Option<Vec<u8>>,
    /// Sprites from the scene's background sprite sheet
    pub sprites: Vec<ChunkBackgroundSprite>,
}

impl ChunkBackgroundUpload {

    /// Prepares a background whose image covers a chunk of a side length, dropping any other image
    pub fn new(background: ChunkBackground, side: usize) -> Self {
        let image: Option<Vec<u8>> = background.image
            .filter(|image| image.len() == side * side)
            .map(|image| {
                let mut bytes: Vec<u8> = Vec::with_capacity(image.len() * 4);
                for row in image.chunks_exact(side).rev() {
                    for pixel in row { bytes.extend_from_slice(&pixel.to_le_bytes()); }
                }
                bytes
            });
        Self { image, sprites: background.sprites }
    }

}
