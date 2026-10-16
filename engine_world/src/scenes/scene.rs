// Copyright Rob Gage 2026

use super::{
    cellular_tiles::{
        CellularTileBuffers,
        CellularTileCoordinates,
        CellularTileRegion,
        CellularTileStreamingState,
        CELLULAR_TILE_SIZE,
    },
    chunks::{
        ChunkData,
        ChunkStreamingState,
    },
    collisions::Collision,
    edits::{
        Edit,
        EditError,
        EditStreamingState,
    },
    kinematics::Kinematics,
    scene_error::SceneError,
    scene_generator::SceneGenerator,
    scene_position::ScenePosition,
    SCENE_CHUNK_THREAD_POOL_NAME,
    SCENE_GRAVITY_DEFAULT,
};
use crate::{
    actors::{
        components::{
            ActorCameraComponent,
            ActorKinematicsPositionComponent,
            ActorSpritesComponent,
        },
        Actor,
        ActorError,
        ActorRegistry,
    },
    materials::MaterialTable,
};
use engine_graphics::world::scenes::{
    SceneGraphics,
    SceneGraphicsCellular,
    SceneGraphicsSprites,
};
use engine_host::{
    acceleration::Accelerator,
    processors::Processor,
};
use std::{
    collections::HashMap,
    sync::Arc,
};

/// A playable scene
pub struct Scene {
    /// Persistent storage of this scene's chunks
    chunk_data: ChunkData,
    /// Handles the streaming of chunks between the disk and native memory, absent until streamed
    chunk_streaming_state: Option<ChunkStreamingState>,
    /// GPU storage for the properties of every resident cellular tile, absent until streamed
    cellular_tile_buffers: Option<CellularTileBuffers>,
    /// GPU tile ring around the active region, absent until a cellular region is requested
    cellular_tile_streaming_state: Option<CellularTileStreamingState>,
    /// Rapier simulation of actors against the active cellular tiles, absent until simulated
    collision: Option<Collision>,
    /// GPU simulation of the active dynamic cells against actors, absent until simulated
    kinematics: Option<Kinematics>,
    /// Requested edits awaiting their write into the GPU-resident cellular tiles
    edit_streaming_state: EditStreamingState,
    /// Downward acceleration of dynamic bodies and pawns in tiles per second squared
    gravity: f32,
    /// ECS world and fixed-tick systems of every live actor
    actor_registry: ActorRegistry,
    /// Materials of every cell in this scene, shared with chunk generation to color new cells
    material_table: Arc<MaterialTable>,
    /// Generates chunks that have no persisted file
    generator: Arc<dyn SceneGenerator>,
    /// Stable seed supplied to the generator
    world_seed: u64,
}

impl Scene {

    /// Creates a scene whose chunks are stored in chunk data or generated when absent
    pub fn new(
        chunk_data: ChunkData,
        material_table: MaterialTable,
        generator: Arc<dyn SceneGenerator>,
        world_seed: u64,
    ) -> Self {
        Self {
            chunk_data,
            chunk_streaming_state: None,
            cellular_tile_buffers: None,
            cellular_tile_streaming_state: None,
            collision: None,
            kinematics: None,
            edit_streaming_state: EditStreamingState::new(),
            gravity: SCENE_GRAVITY_DEFAULT,
            actor_registry: ActorRegistry::new(),
            material_table: Arc::new(material_table),
            generator,
            world_seed,
        }
    }

    /// Returns the actors of this scene for modification
    pub fn actor_registry_mutable(&mut self) -> &mut ActorRegistry { &mut self.actor_registry }

    /// Sets the downward acceleration of dynamic bodies and pawns in tiles per second squared
    pub fn set_gravity(&mut self, gravity: f32) { self.gravity = gravity; }

    /// Returns the compiled materials of this scene
    pub fn material_table(&self) -> &MaterialTable { &self.material_table }

    /// Returns the resident active region, or `None` before it streams
    pub fn active_cellular_tile_region(&self) -> Option<CellularTileRegion> {
        self.cellular_tile_streaming_state.as_ref()?.active_cellular_tile_region()
    }

    /// Requests an edit of resident cells, written before the next stream moves resident tiles
    pub fn edit(&mut self, edit: Edit) -> Result<(), EditError> {
        let active_region: Option<CellularTileRegion> = self.active_cellular_tile_region();
        self.edit_streaming_state.request_edit(edit, &self.material_table, active_region)
    }

    /// Requests that every edit be saved without unloading the resident tiles
    pub fn save(&mut self) {
        if let Some(cellular_tile_streaming_state) = self.cellular_tile_streaming_state.as_mut() {
            cellular_tile_streaming_state.save_cellular_tiles();
        } else if let Some(chunk_streaming_state) = self.chunk_streaming_state.as_mut() {
            chunk_streaming_state.save_chunks();
        }
    }

    /// Returns whether every requested save has completed
    pub fn is_saved(&self) -> bool {
        !self.cellular_tile_streaming_state.as_ref()
            .is_some_and(|state| state.is_cellular_tile_save_requested())
            && !self.chunk_streaming_state.as_ref()
                .is_some_and(|state| state.is_chunk_save_requested())
    }

    /// Streams chunks and the cellular tiles around a camera actor without waiting
    pub fn stream(
        &mut self,
        accelerator: &Accelerator,
        processor: &Processor,
        camera_actor: Actor,
    ) -> Result<(), SceneError> {
        // center a region twice the camera's unzoomed view on the camera
        let camera: &ActorCameraComponent = self.actor_registry.component(camera_actor)
            .map_err(|error: ActorError| SceneError::CameraActor { error })?;
        let region_width: i64 = i64::from(camera.width) * 2;
        let region_height: i64 = i64::from(camera.height) * 2;
        let camera_position: ScenePosition = self.actor_registry
            .component::<ActorKinematicsPositionComponent>(camera_actor)
            .map_err(|error: ActorError| SceneError::CameraActor { error })?
            .position;
        let bottom_left: CellularTileCoordinates = CellularTileCoordinates {
            x: camera_position.tile_x - region_width / 2,
            y: camera_position.tile_y - region_height / 2,
        };
        let region: CellularTileRegion = CellularTileRegion {
            bottom_left,
            top_right: CellularTileCoordinates {
                x: bottom_left.x + region_width - 1,
                y: bottom_left.y + region_height - 1,
            },
        };
        // create chunk streaming and the GPU tile ring when first streamed
        if self.chunk_streaming_state.is_none() {
            let thread_pool: Arc<rayon::ThreadPool> = processor
                .thread_pool(SCENE_CHUNK_THREAD_POOL_NAME)
                .ok_or(SceneError::ChunkThreadPoolUnavailable)?;
            self.chunk_streaming_state = Some(ChunkStreamingState::new(
                self.chunk_data.clone(),
                thread_pool,
                self.material_table.clone(),
            ));
        }
        let Some(chunk_streaming_state) = self.chunk_streaming_state.as_mut()
        else { return Ok(()); };
        let cellular_tile_buffers: &CellularTileBuffers = self.cellular_tile_buffers
            .get_or_insert_with(|| {
                CellularTileBuffers::new(accelerator, region_width as u32, region_height as u32)
            });
        match self.cellular_tile_streaming_state.as_mut() {
            Some(cellular_tile_streaming_state) => {
                cellular_tile_streaming_state.set_cellular_tile_region(region)?;
            }
            None => {
                self.cellular_tile_streaming_state = Some(CellularTileStreamingState::new(
                    cellular_tile_buffers,
                    chunk_streaming_state,
                    region,
                )?);
            }
        }
        // write requested edits before streaming can move the tiles they were requested for
        if let Some(cellular_tile_streaming_state) = self.cellular_tile_streaming_state.as_ref() {
            self.edit_streaming_state.stream_edits(
                accelerator,
                cellular_tile_buffers,
                cellular_tile_streaming_state,
                &self.actor_registry,
                self.collision.as_mut(),
            )?;
        }
        // advance chunk streaming before moving resident tiles
        chunk_streaming_state.stream_chunks(&self.generator, self.world_seed)?;
        // rigid bodies leaving the requested region stay behind as static cells before their
        // tiles can move
        if let (Some(collision), Some(cellular_tile_streaming_state)) =
            (self.collision.as_mut(), self.cellular_tile_streaming_state.as_ref())
            && let Some(active_region) = cellular_tile_streaming_state.active_cellular_tile_region()
        {
            collision.release_rigid_bodies(
                accelerator,
                active_region,
                cellular_tile_streaming_state.cellular_tile_ring_offset(),
                region,
            );
        }
        if let Some(cellular_tile_streaming_state) = self.cellular_tile_streaming_state.as_mut() {
            cellular_tile_streaming_state.stream_cellular_tiles(
                accelerator,
                cellular_tile_buffers,
                chunk_streaming_state,
            )?;
        }
        Ok(())
    }

    /// Simulates the active dynamic cells against actors, then actor and rigid body collisions
    /// against the active cellular tiles, waiting for the GPU only when collisions read changed
    /// cells
    pub fn simulate(&mut self, accelerator: &Accelerator) -> Result<(), SceneError> {
        // nothing is simulated before the active region is resident
        let (Some(cellular_tile_streaming_state), Some(cellular_tile_buffers)) = (
            self.cellular_tile_streaming_state.as_ref(),
            self.cellular_tile_buffers.as_ref(),
        ) else { return Ok(()); };
        let Some(active_region): Option<CellularTileRegion> =
            cellular_tile_streaming_state.active_cellular_tile_region()
        else { return Ok(()); };
        // dynamic cells move against actors where the previous step left them, and push back on
        // actors once their contacts are read
        let cellular_contacts: HashMap<Actor, ([f32; 2], [f32; 2], f32)> = self.kinematics
            .get_or_insert_with(|| {
                Kinematics::new(accelerator, cellular_tile_buffers, &self.material_table)
            })
            .simulate_kinematics(
                accelerator,
                active_region,
                cellular_tile_streaming_state.cellular_tile_ring_offset(),
                self.gravity,
                &self.actor_registry,
            )?;
        self.collision
            .get_or_insert_with(|| Collision::new(accelerator, cellular_tile_buffers))
            .simulate_collisions(
                accelerator,
                active_region,
                cellular_tile_streaming_state.cellular_tile_ring_offset(),
                self.gravity,
                &mut self.actor_registry,
                &cellular_contacts,
                &self.material_table,
            )
    }

    /// Returns the graphical representation of this scene seen through an actor's camera
    pub fn graphics(&self, camera_actor: Actor) -> Result<SceneGraphics<'_>, ActorError> {
        let camera: &ActorCameraComponent = self.actor_registry.component(camera_actor)?;
        let camera_position: ScenePosition = self.actor_registry
            .component::<ActorKinematicsPositionComponent>(camera_actor)?
            .position;
        // place every actor sprite relative to the camera, skipping frames outside their sheet
        // and drawing one sheet pixel per cell at a scale of one
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
                scale_x,
                scale_y,
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
                    sprite_sheet.width() as f32 * *scale_x / CELLULAR_TILE_SIZE as f32,
                    sprite_sheet.height() as f32 * *scale_y / CELLULAR_TILE_SIZE as f32,
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
        let cellular_tile_streaming_state: &CellularTileStreamingState =
            self.cellular_tile_streaming_state.as_ref()?;
        let cellular_tile_buffers: &CellularTileBuffers = self.cellular_tile_buffers.as_ref()?;
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
            material_identifier_buffer: cellular_tile_buffers.material_identifier_buffer(),
            color_buffer: cellular_tile_buffers.color_buffer(),
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

impl Drop for Scene {

    fn drop(&mut self) {
        // write requested edits and return every active tile so the dropped chunks save them
        let (
            Some(chunk_streaming_state),
            Some(cellular_tile_buffers),
            Some(cellular_tile_streaming_state),
        ) = (
            self.chunk_streaming_state.as_mut(),
            self.cellular_tile_buffers.as_ref(),
            self.cellular_tile_streaming_state.as_mut(),
        ) else { return; };
        let accelerator: &Accelerator = cellular_tile_buffers.accelerator();
        let result: Result<(), SceneError> = self.edit_streaming_state
            .stream_edits(
                accelerator,
                cellular_tile_buffers,
                cellular_tile_streaming_state,
                &self.actor_registry,
                None,
            )
            .and_then(|()| cellular_tile_streaming_state.return_cellular_tiles(
                accelerator,
                cellular_tile_buffers,
                chunk_streaming_state,
            ));
        if let Err(error) = result {
            panic!("Scene cellular tiles could not be returned: {error:?}");
        }
    }

}
