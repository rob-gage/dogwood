// Copyright Rob Gage 2026

use super::{
    scene_cellular_tile_streaming_state::SceneCellularTileStreamingState,
    scene_chunk_streaming_state::SceneChunkStreamingState,
    scene_position::ScenePosition,
};
use crate::{
    actor_components::{
        ActorCameraComponent,
        ActorKinematicsPositionComponent,
        ActorSpritesComponent,
    },
    actors::{
        Actor,
        ActorError,
        ActorRegistry,
    },
    cellular_tiles::{
        CellularTileBuffers,
        CellularTileRegion,
        CELLULAR_TILE_SIZE,
    },
    chunks::ChunkData,
};
use engine_graphics::world::scenes::{
    SceneGraphics,
    SceneGraphicsCellular,
    SceneGraphicsSprites,
};
use engine_host::acceleration::Accelerator;
use std::sync::Arc;

/// A playable scene
pub struct Scene {
    /// Handles the streaming of chunks between the disk and native memory
    chunk_streaming_state: SceneChunkStreamingState,
    /// GPU storage for the properties of every resident cellular tile
    cellular_tile_buffers: CellularTileBuffers,
    /// GPU tile ring around the active region, absent until a cellular region is requested
    cellular_tile_streaming_state: Option<SceneCellularTileStreamingState>,
    /// ECS world and fixed-tick systems of every live actor
    actor_registry: ActorRegistry,
}

impl Scene {

    /// Creates a scene with chunk storage, a shared Rayon pool, and cellular tile buffers
    pub fn new(
        chunk_data: ChunkData,
        thread_pool: Arc<rayon::ThreadPool>,
        accelerator: &Accelerator,
        width: u32,
        height: u32,
    ) -> Self {
        let chunk_streaming_state: SceneChunkStreamingState =
            SceneChunkStreamingState::new(chunk_data, thread_pool);
        let cellular_tile_buffers: CellularTileBuffers =
            CellularTileBuffers::new(accelerator, width, height);
        let actor_registry: ActorRegistry = ActorRegistry::new();
        Self {
            chunk_streaming_state,
            cellular_tile_buffers,
            cellular_tile_streaming_state: None,
            actor_registry,
        }
    }

    /// Returns the graphical representation of this scene seen through an actor's camera
    pub fn graphics(&self, camera_actor: Actor) -> Result<SceneGraphics<'_>, ActorError> {
        let camera: &ActorCameraComponent = self.actor_registry.component(camera_actor)?;
        let camera_position: ScenePosition = self.actor_registry
            .component::<ActorKinematicsPositionComponent>(camera_actor)?
            .position;
        // place every actor sprite relative to the camera, skipping frames outside their sheet
        let mut sprites: SceneGraphicsSprites<'_> = SceneGraphicsSprites::default();
        for (actor_sprites, position) in self.actor_registry
            .components::<ActorSpritesComponent, ActorKinematicsPositionComponent>()
        {
            let (actor_x, actor_y): (f64, f64) =
                position.position.relative_position(camera_position);
            for (
                sprite_sheet,
                frame,
                offset_x,
                offset_y,
                width,
                height,
                rotation,
                color,
                depth,
            ) in &actor_sprites.sprites {
                let Some(texture_region): Option<[f32; 4]> =
                    sprite_sheet.frame_texture_region(*frame)
                else { continue; };
                sprites.sprites.push((
                    sprite_sheet.texture_view(),
                    (actor_x + f64::from(*offset_x)) as f32,
                    (actor_y + f64::from(*offset_y)) as f32,
                    *width,
                    *height,
                    *rotation,
                    *depth,
                    texture_region,
                    *color,
                ));
            }
        }
        Ok(SceneGraphics {
            visible_width: camera.visible_width(),
            visible_height: camera.visible_height(),
            cellular: self.cellular_graphics(camera_position),
            sprites,
        })
    }

    /// Returns the resident active region relative to a camera, or `None` before it streams
    pub fn cellular_graphics(
        &self,
        camera_position: ScenePosition,
    ) -> Option<SceneGraphicsCellular<'_>> {
        let cellular_tile_streaming_state: &SceneCellularTileStreamingState =
            self.cellular_tile_streaming_state.as_ref()?;
        let active_region: CellularTileRegion =
            cellular_tile_streaming_state.active_cellular_tile_region()?;
        let (ring_offset_x, ring_offset_y): (u32, u32) =
            cellular_tile_streaming_state.cellular_tile_ring_offset();
        let region_position: ScenePosition = ScenePosition {
            tile_x: active_region.bottom_left.x,
            tile_y: active_region.bottom_left.y,
            offset_x: 0,
            offset_y: 0,
        };
        let (region_x, region_y): (f64, f64) = region_position.relative_position(camera_position);
        Some(SceneGraphicsCellular {
            material_identifier_buffer: self.cellular_tile_buffers.material_identifier_buffer(),
            color_buffer: self.cellular_tile_buffers.color_buffer(),
            region_x: region_x as f32,
            region_y: region_y as f32,
            region_width: active_region.width() as u32,
            region_height: active_region.height() as u32,
            ring_offset_x,
            ring_offset_y,
            cellular_tile_size: CELLULAR_TILE_SIZE as u32,
        })
    }

}
