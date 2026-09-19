// Copyright Rob Gage 2026

use engine::physics::{
    actors::{ActorCollisionShape, ActorPhysicalConfiguration, ActorPhysicalSpawn},
    chunks::{Chunk, ChunkGenerationRegion, ChunkInitializationWriter},
    materials::MaterialIdentifier,
    scenes::{SceneGenerator, ScenePosition, SceneVelocity},
    tiles::{CellCoordinates, CellularAppearance, TileCoordinates},
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

    fn generate_chunk(
        &self,
        _: u128,
        region: ChunkGenerationRegion,
        initialization: &mut ChunkInitializationWriter<'_>,
    ) {
        let origin: CellCoordinates = region.cell_origin;
        for y in origin.y..origin.y + i32::from(region.cell_dimensions[1]) {
            if y >= 0 {
                continue;
            }
            for x in origin.x..origin.x + i32::from(region.cell_dimensions[0]) {
                initialization.set_cell_with_integrity(
                    CellCoordinates { x, y },
                    self.stone,
                    CellularAppearance::from_seed(
                        CellCoordinates { x, y }.appearance_seed(),
                        self.stone_variation,
                    ),
                    self.stone_integrity,
                );
            }
        }
    }
}
