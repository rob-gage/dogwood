// Copyright Rob Gage 2026

use engine::physics::{
    actors::{ActorCollisionShape, ActorPhysicalConfiguration, ActorPhysicalSpawn},
    chunks::Chunk,
    materials::MaterialIdentifier,
    scenes::{SceneGenerator, ScenePosition, SceneVelocity},
    tiles::{CellCoordinates, CellularAppearance, TileCoordinates, TileData},
};

/// Generates flat stone ground.
pub(crate) struct TemplateSceneGenerator {
    pub(crate) stone: MaterialIdentifier,
    pub(crate) stone_variation: [f32; 4],
    pub(crate) stone_integrity: f32,
}

impl SceneGenerator for TemplateSceneGenerator {
    fn generate_actor_spawns_with_seed(
        &self,
        _seed: u128,
        coordinates: TileCoordinates,
    ) -> Vec<ActorPhysicalSpawn> {
        if coordinates.y != -64 {
            return Vec::new();
        }
        (coordinates.x..coordinates.x + i32::from(Chunk::WIDTH))
            .filter(|x| x.rem_euclid(8) == 0)
            .map(|x| {
                let jitter = ((x.unsigned_abs() as f32 * 0.754_877_7).sin()) * 0.18;
                ActorPhysicalSpawn {
                    configuration: ActorPhysicalConfiguration {
                        collision_shape: ActorCollisionShape::Rectangle {
                            width: 0.7,
                            height: 0.7,
                        },
                        color: crate::actors::DEMO_SQUARE_COLOR,
                        ..Default::default()
                    },
                    position: ScenePosition {
                        tile_coordinates: TileCoordinates { x, y: 2 },
                        x_offset: 0.5 + jitter,
                        y_offset: 0.5,
                    },
                    velocity: SceneVelocity { x: 0.0, y: 0.0 },
                }
            })
            .collect()
    }

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
                        let material: Option<(MaterialIdentifier, [f32; 4])> =
                            (world_cell_y < 0).then_some((self.stone, self.stone_variation));
                        let Some((material_identifier, variation)) = material else {
                            continue;
                        };
                        let integrity: f32 = if material_identifier == self.stone {
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
