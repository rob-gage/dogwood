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

/// Generates flat stone ground extending infinitely along the horizontal axis
pub(super) struct DemoSceneGenerator {
    pub(super) stone: MaterialIdentifier,
}

impl SceneGenerator for DemoSceneGenerator {

    fn generate_chunk_with_seed(&self, _: u128, coordinates: TileCoordinates) -> Chunk {
        let mut chunk: Chunk = Chunk::new_empty(coordinates);
        for y in 0..Chunk::WIDTH {
            let tile_y: i32 = coordinates.y + i32::from(y);
            for x in 0..Chunk::WIDTH {
                let tile_x: i32 = coordinates.x + i32::from(x);
                if tile_y >= 0 { continue; }
                chunk.set_tile_unchecked(TileCoordinates {
                    x: tile_x,
                    y: tile_y,
                }, TileData::new_filled(self.stone));
            }
        }
        chunk
    }

}
