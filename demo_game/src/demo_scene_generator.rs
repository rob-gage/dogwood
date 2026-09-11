// Copyright Rob Gage 2026

use engine::physics::{
    chunks::Chunk,
    materials::MaterialIdentifier,
    scenes::SceneGenerator,
    tiles::{
        CellularAppearance,
        CellCoordinates,
        TileCoordinates,
        TileData,
    },
};

/// Generates flat stone ground extending infinitely along the horizontal axis
pub(super) struct DemoSceneGenerator {
    pub(super) stone: MaterialIdentifier,
    pub(super) stone_variation: [f32; 4],
}

impl SceneGenerator for DemoSceneGenerator {

    fn generate_chunk_with_seed(&self, _: u128, coordinates: TileCoordinates) -> Chunk {
        let mut chunk: Chunk = Chunk::new_empty(coordinates);
        for y in 0..Chunk::WIDTH {
            let tile_y: i32 = coordinates.y + i32::from(y);
            for x in 0..Chunk::WIDTH {
                let tile_x: i32 = coordinates.x + i32::from(x);
                if tile_y >= 0 { continue; }
                let mut tile: TileData = TileData::EMPTY;
                for cell_y in 0..8 {
                    for cell_x in 0..8 {
                        let world_cell_x: i32 = tile_x * 8 + cell_x;
                        let world_cell_y: i32 = tile_y * 8 + cell_y;
                        tile.set_cell(
                            cell_x as usize,
                            cell_y as usize,
                            self.stone,
                            CellularAppearance::from_seed(
                                CellCoordinates { x: world_cell_x, y: world_cell_y }
                                    .appearance_seed(),
                                self.stone_variation,
                            ),
                        );
                    }
                }
                chunk.set_tile_unchecked(TileCoordinates { x: tile_x, y: tile_y }, tile);
            }
        }
        chunk
    }

}
