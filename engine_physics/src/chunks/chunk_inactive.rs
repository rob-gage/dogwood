// Copyright Rob Gage 2026

/// An inactive chunk that can be loaded into a scene
pub struct ChunkInactive {
    /// The tiles in this `InactiveChunk`
    ///
    /// Tiles in the chunk are positioned in the array as shown below:
    ///
    ///      0  1  2  3
    ///      4  5  6  7
    ///      8  9 10 11
    ///     12 13 14 15
    tiles: [(); 16],
}