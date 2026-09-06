// Copyright Rob Gage 2026

use engine::physics::{
    chunks::Chunk,
    materials::MaterialIdentifier,
    scenes::SceneGenerator,
    tiles::{
        TileCoordinates,
        TileData,
    },
};

/// Generates empty sky above a stone ground
pub(super) struct DemoSceneGenerator {
    pub(super) stone: MaterialIdentifier,
}

impl SceneGenerator for DemoSceneGenerator {

    fn generate_chunk_with_seed(&self, _: u128, coordinates: TileCoordinates) -> Chunk {
        let mut chunk: Chunk = Chunk::new_empty(coordinates);
        if coordinates.y >= 0 { return chunk; }
        for y in 0..Chunk::WIDTH {
            for x in 0..Chunk::WIDTH {
                chunk.set_tile_unchecked(TileCoordinates {
                    x: coordinates.x + i32::from(x),
                    y: coordinates.y + i32::from(y),
                }, TileData::new_filled(self.stone));
            }
        }
        chunk
    }

}
