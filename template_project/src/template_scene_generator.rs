// Copyright Rob Gage 2026

use engine::physics::{
    chunks::Chunk,
    materials::MaterialIdentifier,
    scenes::SceneGenerator,
    tiles::{CellCoordinates, CellularAppearance, TileCoordinates, TileData},
};

/// Generates flat stone ground with a suspended Stone Debris test mass
pub(super) struct TemplateSceneGenerator {
    pub(super) stone: MaterialIdentifier,
    pub(super) stone_variation: [f32; 4],
    pub(super) sand: MaterialIdentifier,
    pub(super) sand_variation: [f32; 4],
    pub(super) stone_integrity: f32,
}

impl SceneGenerator for TemplateSceneGenerator {
    fn generate_chunk_with_seed(&self, _: u128, coordinates: TileCoordinates) -> Chunk {
        let mut chunk: Chunk = Chunk::new_empty(coordinates);
        for y in 0..Chunk::WIDTH {
            let tile_y: i32 = coordinates.y + i32::from(y);
            for x in 0..Chunk::WIDTH {
                let tile_x: i32 = coordinates.x + i32::from(x);
                let mut tile: TileData = TileData::EMPTY;
                let mut has_cells: bool = false;
                for cell_y in 0..8 {
                    for cell_x in 0..8 {
                        let world_cell_x: i32 = tile_x * 8 + cell_x;
                        let world_cell_y: i32 = tile_y * 8 + cell_y;
                        let material: Option<(MaterialIdentifier, [f32; 4])> = if world_cell_y < 0 {
                            Some((self.stone, self.stone_variation))
                        } else if (16..40).contains(&world_cell_x)
                            && (28..40).contains(&world_cell_y)
                        {
                            Some((self.sand, self.sand_variation))
                        } else if (48..56).contains(&world_cell_x)
                            && (0..24).contains(&world_cell_y)
                        {
                            Some((self.stone, self.stone_variation))
                        } else {
                            None
                        };
                        let Some((material_identifier, variation)) = material else {
                            continue;
                        };
                        let integrity = if material_identifier == self.stone {
                            self.stone_integrity
                        } else {
                            0.0
                        };
                        tile.set_cell_with_integrity(
                            cell_x as usize,
                            cell_y as usize,
                            material_identifier,
                            CellularAppearance::from_seed(
                                CellCoordinates {
                                    x: world_cell_x,
                                    y: world_cell_y,
                                }
                                .appearance_seed(),
                                variation,
                            ),
                            integrity,
                        );
                        has_cells = true;
                    }
                }
                if has_cells {
                    chunk.set_tile_unchecked(
                        TileCoordinates {
                            x: tile_x,
                            y: tile_y,
                        },
                        tile,
                    );
                }
            }
        }
        chunk
    }
}
