// Copyright Rob Gage 2026

use super::{
    cellular_tiles::{
        CellularTileBuffers,
        CellularTileCoordinates,
        CellularTileRegion,
        CellularTileStreamingState,
        CellularTiles,
        CELLULAR_TILE_SIZE,
    },
    chunks::{
        ChunkCoordinates,
        ChunkData,
        ChunkFluidParticle,
        ChunkRegionIndex,
        ChunkStreamingState,
    },
    collisions::Collision,
    edits::{
        Edit,
        EditError,
        EditStreamingState,
    },
    fluids::{
        FluidActorContact,
        Fluids,
    },
    pressure::{
        Pressure,
        GAS_AMOUNT_MAXIMUM,
        GAS_AMOUNT_PER_ATMOSPHERE,
    },
    scene_error::SceneError,
    scene_generator::SceneGenerator,
    thermal::Thermal,
    scene_position::ScenePosition,
    SCENE_AMBIENT_GAS_CONDUCTION_DEFAULT,
    SCENE_AMBIENT_GAS_HEAT_CAPACITY_DEFAULT,
    SCENE_AMBIENT_GAS_MASS_DEFAULT,
    SCENE_AMBIENT_GAS_PRESSURE_DEFAULT,
    SCENE_AMBIENT_GAS_TEMPERATURE_DEFAULT,
    SCENE_CHUNK_THREAD_POOL_NAME,
    SCENE_FLUID_CAPACITY_FACTOR_DEFAULT,
    SCENE_GRAVITY_DEFAULT,
};
use crate::{
    actors::{
        components::{
            ActorCameraComponent,
            ActorCollisionComponent,
            ActorKinematicsPositionComponent,
            ActorOverlayComponent,
            ActorOverlayShape,
            ActorParallaxComponent,
            ActorPawnControlComponent,
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
    SceneGraphicsFluid,
    SceneGraphicsGas,
    SceneGraphicsOverlay,
    SceneGraphicsOverlayShape,
    SceneGraphicsParallax,
    SceneGraphicsPressure,
    SceneGraphicsSprites,
};
use engine_host::{
    acceleration::Accelerator,
    processors::Processor,
};
use std::{
    collections::{
        BTreeSet,
        HashMap,
        HashSet,
    },
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
    cellular_tiles: Option<CellularTiles>,
    /// GPU PBF simulation and fixed-capacity particle pool, absent until the tile ring is created
    fluids: Option<Fluids>,
    /// GPU simulation of the gas in the active cellular tiles, absent until simulated
    pressure: Option<Pressure>,
    /// GPU simulation of the heat in the active cellular tiles, absent until simulated
    thermal: Option<Thermal>,
    /// Requested edits awaiting their write into the GPU-resident cellular tiles
    edit_streaming_state: EditStreamingState,
    /// Whether the next stream advance must start a resident particle save snapshot
    is_fluid_save_requested: bool,
    /// Chunk retention protecting a particle snapshot until it reaches chunk storage
    fluid_snapshot_chunk_region_index: Option<ChunkRegionIndex>,
    /// Tile region whose live particles are held by the pending snapshot
    fluid_snapshot_region: Option<CellularTileRegion>,
    /// Downward acceleration of dynamic bodies and pawns in tiles per second squared
    gravity: f32,
    /// Fraction of active-region cells available to fluid particles
    fluid_capacity_factor: f32,
    /// Pressure of the phantom air in empty cells in atmospheres
    ambient_gas_pressure: f32,
    /// Mass of one atmosphere of the phantom air in empty cells
    ambient_gas_mass: f32,
    /// Temperature in kelvin of the phantom air in empty cells, which open cells relax toward
    ambient_gas_temperature: f32,
    /// Heat per atmosphere of the phantom air in one cell needed to warm it by one kelvin
    ambient_gas_heat_capacity: f32,
    /// Heat per second per kelvin passing through each face between cells of phantom air, and
    /// between phantom air and its surroundings
    ambient_gas_conduction: f32,
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
            cellular_tiles: None,
            fluids: None,
            pressure: None,
            thermal: None,
            edit_streaming_state: EditStreamingState::new(),
            is_fluid_save_requested: false,
            fluid_snapshot_chunk_region_index: None,
            fluid_snapshot_region: None,
            gravity: SCENE_GRAVITY_DEFAULT,
            fluid_capacity_factor: SCENE_FLUID_CAPACITY_FACTOR_DEFAULT,
            ambient_gas_pressure: SCENE_AMBIENT_GAS_PRESSURE_DEFAULT,
            ambient_gas_mass: SCENE_AMBIENT_GAS_MASS_DEFAULT,
            ambient_gas_temperature: SCENE_AMBIENT_GAS_TEMPERATURE_DEFAULT,
            ambient_gas_heat_capacity: SCENE_AMBIENT_GAS_HEAT_CAPACITY_DEFAULT,
            ambient_gas_conduction: SCENE_AMBIENT_GAS_CONDUCTION_DEFAULT,
            actor_registry: ActorRegistry::new(),
            material_table: Arc::new(material_table),
            generator,
            world_seed,
        }
    }

    /// Returns the actors of this scene for modification
    pub fn actor_registry_mutable(&mut self) -> &mut ActorRegistry { &mut self.actor_registry }

    /// Returns the edges of every collider in the collision world as line segments for debugging
    pub fn collider_outlines(&self) -> Vec<(ScenePosition, ScenePosition)> {
        self.collision.as_ref().map_or_else(Vec::new, Collision::collider_outlines)
    }

    /// Sets the downward acceleration of dynamic bodies and pawns in tiles per second squared
    pub fn set_gravity(&mut self, gravity: f32) { self.gravity = gravity; }

    /// Sets capacity to `floor(factor * active-region cell count)` particles before the pool exists
    pub fn set_fluid_capacity_factor(&mut self, factor: f32) -> Result<(), SceneError> {
        if self.fluids.is_some() { return Err(SceneError::FluidCapacityFixed); }
        if !factor.is_finite() || !(0.0..=1.0).contains(&factor) || factor == 0.0 {
            return Err(SceneError::InvalidFluidCapacityFactor);
        }
        self.fluid_capacity_factor = factor;
        Ok(())
    }

    /// Returns the configured fraction of active-region cells available to fluid particles
    pub fn fluid_capacity_factor(&self) -> f32 { self.fluid_capacity_factor }

    /// Sets the pressure in atmospheres of the phantom air filling empty cells, which tiles without
    /// stored gas and emptied cells start with; zero leaves empty cells a vacuum
    pub fn set_ambient_gas_pressure(&mut self, pressure: f32) -> Result<(), SceneError> {
        if !pressure.is_finite() || !(0.0..256.0).contains(&pressure) {
            return Err(SceneError::InvalidAmbientGasPressure);
        }
        self.ambient_gas_pressure = pressure;
        Ok(())
    }

    /// Sets the mass of one atmosphere of the phantom air filling empty cells: gases with less mass
    /// rise through it and gases with more sink
    pub fn set_ambient_gas_mass(&mut self, mass: f32) -> Result<(), SceneError> {
        if !mass.is_finite() || mass <= 0.0 { return Err(SceneError::InvalidAmbientGasMass); }
        self.ambient_gas_mass = mass;
        Ok(())
    }

    /// Returns the configured ambient gas temperature in kelvin
    pub fn ambient_gas_temperature(&self) -> f32 { self.ambient_gas_temperature }

    /// Sets the temperature in kelvin of the phantom air filling empty cells, which tiles without
    /// stored temperatures start with and open cells relax toward
    pub fn set_ambient_gas_temperature(&mut self, temperature: f32) -> Result<(), SceneError> {
        if !temperature.is_finite() || temperature < 0.0 {
            return Err(SceneError::InvalidAmbientGasTemperature);
        }
        self.ambient_gas_temperature = temperature;
        Ok(())
    }

    /// Sets the heat per atmosphere of the phantom air in one cell needed to warm it by one kelvin
    pub fn set_ambient_gas_heat_capacity(&mut self, heat_capacity: f32) -> Result<(), SceneError> {
        if !heat_capacity.is_finite() || heat_capacity <= 0.0 {
            return Err(SceneError::InvalidAmbientGasHeatCapacity);
        }
        self.ambient_gas_heat_capacity = heat_capacity;
        Ok(())
    }

    /// Sets the heat per second per kelvin passing through each face between cells of phantom air,
    /// and between phantom air and the surroundings at the ambient gas temperature it relaxes
    /// toward; zero keeps empty cells from conducting or relaxing heat
    pub fn set_ambient_gas_conduction(&mut self, conduction: f32) -> Result<(), SceneError> {
        if !conduction.is_finite() || conduction < 0.0 {
            return Err(SceneError::InvalidAmbientGasConduction);
        }
        self.ambient_gas_conduction = conduction;
        Ok(())
    }

    /// Returns the phantom air amount of one empty cell at the ambient gas pressure
    fn ambient_gas_amount(&self) -> u32 {
        ((self.ambient_gas_pressure * GAS_AMOUNT_PER_ATMOSPHERE).round() as u32)
            .min(GAS_AMOUNT_MAXIMUM)
    }

    /// Returns the compiled materials of this scene
    pub fn material_table(&self) -> &MaterialTable { &self.material_table }

    /// Returns the resident active region, or `None` before it streams
    pub fn active_cellular_tile_region(&self) -> Option<CellularTileRegion> {
        self.cellular_tile_streaming_state.as_ref()?.active_cellular_tile_region()
    }

    /// Returns the unique chunks overlapped by a tile region in row-major order
    fn chunk_coordinates_in_region(
        region: CellularTileRegion,
    ) -> Vec<ChunkCoordinates> {
        let mut coordinates: BTreeSet<ChunkCoordinates> = BTreeSet::new();
        for tile in region.cellular_tile_coordinates() {
            coordinates.insert(tile.chunk_coordinates());
        }
        coordinates.into_iter().collect()
    }

    /// Requests an edit of resident cells, written before the next stream moves resident tiles
    pub fn edit(&mut self, edit: Edit) -> Result<(), EditError> {
        let active_region: Option<CellularTileRegion> = self.active_cellular_tile_region();
        if self.fluids.as_ref().is_some_and(|fluids| !fluids.can_queue_edit(&edit)) {
            return Err(EditError::CapacityReached);
        }
        let queued_edit: &Edit = self.edit_streaming_state.request_edit(
            edit,
            &self.material_table,
            active_region,
            self.ambient_gas_temperature,
        )?;
        if let Some(fluids) = self.fluids.as_mut() { fluids.queue_edit(queued_edit); }
        Ok(())
    }

    /// Requests that every edit be saved without unloading the resident tiles
    pub fn save(&mut self) {
        if let Some(cellular_tile_streaming_state) = self.cellular_tile_streaming_state.as_mut() {
            cellular_tile_streaming_state.save_cellular_tiles();
            if self.fluids.is_some() { self.is_fluid_save_requested = true; }
        } else if let Some(chunk_streaming_state) = self.chunk_streaming_state.as_mut() {
            chunk_streaming_state.save_chunks();
        }
    }

    /// Returns whether every requested save has completed
    pub fn is_saved(&self) -> bool {
        !self.cellular_tile_streaming_state.as_ref()
            .is_some_and(|state| state.is_cellular_tile_save_requested())
            && !self.is_fluid_save_requested
            && !self.fluids.as_ref()
                .is_some_and(|fluids| fluids.has_pending_particle_snapshot())
            && self.fluid_snapshot_region.is_none()
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
        let ambient_gas_amount: u32 = self.ambient_gas_amount();
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
        self.fluids.get_or_insert_with(|| Fluids::new(
            accelerator,
            cellular_tile_buffers,
            &self.material_table,
            self.fluid_capacity_factor,
            region_width as u32,
            region_height as u32,
        ));
        match self.cellular_tile_streaming_state.as_mut() {
            Some(cellular_tile_streaming_state) => {
                cellular_tile_streaming_state.set_cellular_tile_region(region)?;
            }
            None => {
                self.cellular_tile_streaming_state = Some(CellularTileStreamingState::new(
                    cellular_tile_buffers,
                    chunk_streaming_state,
                    region,
                    self.fluids.as_ref()
                        .expect("fluid storage exists with the tile buffers")
                        .particle_export_buffer_size(),
                )?);
            }
        }
        let fluids: &mut Fluids = self.fluids.as_mut()
            .expect("fluid storage exists with the tile buffers");
        let rejected_imports: Vec<(ChunkCoordinates,
            ChunkFluidParticle)> =
            fluids.poll_rejected_imports(accelerator)?;
        if let Err(error) = chunk_streaming_state.append_fluid_particles(rejected_imports.clone()) {
            fluids.restore_rejected_imports(rejected_imports);
            return Err(error);
        }
        if let Some(snapshot_particles) = fluids.read_particle_snapshot(accelerator)? {
            let snapshot_region: CellularTileRegion = self.fluid_snapshot_region
                .ok_or(SceneError::FluidParticleExportRecordInvalid)?;
            let snapshot_chunks: Vec<ChunkCoordinates> =
                Self::chunk_coordinates_in_region(snapshot_region);
            chunk_streaming_state.replace_resident_fluid_particle_snapshot(
                &snapshot_chunks,
                snapshot_particles,
            )?;
            if let Some(chunk_region_index) = self.fluid_snapshot_chunk_region_index.take() {
                chunk_streaming_state.release_chunk_region_request(chunk_region_index)?;
            }
            self.fluid_snapshot_region = None;
        }
        if self.is_fluid_save_requested && !fluids.has_pending_particle_imports()
            && let Some(active_region) = self.cellular_tile_streaming_state.as_ref()
                .and_then(CellularTileStreamingState::active_cellular_tile_region)
        {
            let snapshot_region: CellularTileRegion =
                fluids.particle_snapshot_region(active_region);
            let chunk_region_index: ChunkRegionIndex =
                chunk_streaming_state.retain_chunk_region(snapshot_region.chunk_region())?;
            if fluids.request_particle_snapshot(accelerator, snapshot_region) {
                self.fluid_snapshot_chunk_region_index = Some(chunk_region_index);
                self.fluid_snapshot_region = Some(snapshot_region);
                self.is_fluid_save_requested = false;
            } else {
                chunk_streaming_state.release_chunk_region_request(chunk_region_index)?;
            }
        }
        let is_particle_snapshot_pending: bool = self.is_fluid_save_requested
            || fluids.has_pending_particle_snapshot()
            || fluids.has_pending_particle_imports();
        // write requested edits before streaming can move the tiles they were requested for
        if let Some(cellular_tile_streaming_state) = self.cellular_tile_streaming_state.as_ref() {
            self.edit_streaming_state.stream_edits(
                accelerator,
                cellular_tile_buffers,
                cellular_tile_streaming_state,
                &self.actor_registry,
                self.collision.as_mut(),
                ambient_gas_amount,
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
                fluids,
                is_particle_snapshot_pending,
                ambient_gas_amount,
                self.ambient_gas_temperature,
            )?;
        }
        Ok(())
    }

    /// Simulates the active dynamic cells against actors and rigid bodies, then actor and rigid
    /// body collisions against the active cellular tiles, waiting for the GPU when
    /// collisions read settled occupancies or changed cells
    pub fn simulate(&mut self, accelerator: &Accelerator) -> Result<(), SceneError> {
        // nothing is simulated before the active region is resident
        let (Some(cellular_tile_streaming_state), Some(cellular_tile_buffers)) = (
            self.cellular_tile_streaming_state.as_ref(),
            self.cellular_tile_buffers.as_ref(),
        ) else { return Ok(()); };
        let Some(active_region): Option<CellularTileRegion> =
            cellular_tile_streaming_state.active_cellular_tile_region()
        else { return Ok(()); };
        let ambient_gas_amount: u32 = self.ambient_gas_amount();
        // dynamic cells move against actors and rigid bodies where the previous step left them,
        // and push back on them once their contacts are read
        let collision: &mut Collision = self.collision
            .get_or_insert_with(|| Collision::new(accelerator, cellular_tile_buffers));
        let origin: CellularTileCoordinates = collision.origin();
        let region_origin: (i32, i32) = (
            ((active_region.bottom_left.x - origin.x) * CELLULAR_TILE_SIZE) as i32,
            ((active_region.bottom_left.y - origin.y) * CELLULAR_TILE_SIZE) as i32,
        );
        let fluid_raster_buffer: &wgpu::Buffer = self.fluids
            .as_ref()
            .expect("fluid storage exists with the tile buffers")
            .fluid_raster_buffer();
        // only idle swimmers drag water along: a stroking swimmer would otherwise carry the water
        // its speed is measured against and outrun its swimming speed
        let swimming_actors: HashSet<Actor> = collision.swimming_actors()
            .into_iter()
            .filter(|actor| self.actor_registry
                .component::<ActorPawnControlComponent>(*actor)
                .map_or(true, |control| control.movement == 0.0
                    && !control.is_jumping
                    && !control.is_crouching))
            .collect();
        let (cellular_contacts, rigid_body_contacts): (
            HashMap<Actor, ([f32; 2], [f32; 2], f32)>,
            HashMap<u64, ([f32; 2], f32, [f32; 7])>,
        ) = self.cellular_tiles
            .get_or_insert_with(|| CellularTiles::new(
                accelerator,
                cellular_tile_buffers,
                &self.material_table,
                collision.rigid_body_pass(),
                fluid_raster_buffer,
            ))
            .simulate_cellular_tiles(
                accelerator,
                active_region,
                cellular_tile_streaming_state.cellular_tile_ring_offset(),
                region_origin,
                self.gravity,
                &self.actor_registry,
                &swimming_actors,
                &collision.rigid_bodies(),
                &collision.rigid_body_fluid_tiles(
                    active_region,
                    cellular_tile_streaming_state.cellular_tile_ring_offset(),
                ),
            )?;
        let actor_components: &[(Actor, ActorCollisionComponent)] =
            self.cellular_tiles.as_ref()
                .expect("cellular tile simulation exists after simulation")
                .fluid_actors();
        let particle_export_target: Option<(u64, &wgpu::Buffer)> =
            cellular_tile_streaming_state.pending_fluid_particle_export_target();
        let fluid_actor_contacts: HashMap<Actor, FluidActorContact> = self.fluids.as_mut()
            .expect("fluid storage exists with the tile buffers")
            .poll_actor_contacts(accelerator, actor_components)?;
        collision.simulate_collisions(
            accelerator,
            active_region,
            cellular_tile_streaming_state.cellular_tile_ring_offset(),
            self.gravity,
            &mut self.actor_registry,
            &cellular_contacts,
            &rigid_body_contacts,
            &fluid_actor_contacts,
            &self.material_table,
        )?;
        let fluid_heat_buffer: &wgpu::Buffer = self.fluids
            .as_ref()
            .expect("fluid storage exists with the tile buffers")
            .fluid_heat_buffer();
        let thermal: &mut Thermal = self.thermal.get_or_insert_with(|| Thermal::new(
            accelerator,
            cellular_tile_buffers,
            &self.material_table,
            fluid_heat_buffer,
            collision.rigid_body_pass().cellular_tile_buffer(),
            collision.changed_cellular_tile_buffer(),
            collision.changed_rigid_body_cell_buffer(),
        ));
        let thermal_has_fluid_conversions: bool = thermal.has_fluid_conversions(accelerator)?;
        let is_converting: bool = self.material_table.has_fluid_transitions()
            && (self.fluids.as_ref()
                .expect("fluid storage exists with the tile buffers")
                .has_pending_fluid_conversions()
                || thermal_has_fluid_conversions);
        let conversion_buffer: &wgpu::Buffer = thermal.conversion_buffer();
        let cellular_tiles: &CellularTiles = self.cellular_tiles
            .as_ref()
            .expect("cellular tile simulation exists after simulation");
        let cellular_tile_actor_buffer: &wgpu::Buffer = cellular_tiles.actor_buffer();
        let particle_simulation_result: Result<(), SceneError> =
            self.fluids.as_mut().expect("fluid storage exists with the tile buffers")
            .simulate(
                accelerator,
                active_region,
                cellular_tile_streaming_state.cellular_tile_ring_offset(),
                self.gravity,
                region_origin,
                cellular_tile_buffers,
                collision.rigid_body_pass(),
                Some(cellular_tile_actor_buffer),
                actor_components,
                particle_export_target.as_ref().map(|(_, buffer)| *buffer),
                self.material_table.fluid_properties_buffer(),
                &self.material_table,
                conversion_buffer,
                collision.changed_rigid_body_cell_buffer(),
                is_converting,
            );
        particle_simulation_result?;
        // pressure moves last, through the cells the other simulations and actors left open
        let fluid_raster_buffer: &wgpu::Buffer = self.fluids
            .as_ref()
            .expect("fluid storage exists with the tile buffers")
            .fluid_raster_buffer();
        let cellular_tiles: &CellularTiles = self.cellular_tiles
            .as_ref()
            .expect("cellular tile simulation exists after simulation");
        let pressure: &mut Pressure = self.pressure
            .get_or_insert_with(|| Pressure::new(
                accelerator,
                cellular_tile_buffers,
                &self.material_table,
                fluid_raster_buffer,
                cellular_tiles.actor_buffer(),
                collision.rigid_body_pass().cellular_tile_buffer(),
                collision.changed_cellular_tile_buffer(),
                collision.changed_rigid_body_cell_buffer(),
            ));
        pressure.simulate_pressure(
            accelerator,
            active_region,
            cellular_tile_streaming_state.cellular_tile_ring_offset(),
            self.gravity,
            ambient_gas_amount,
            self.ambient_gas_mass,
            self.ambient_gas_temperature,
            self.ambient_gas_heat_capacity,
            cellular_tiles.fluid_actors().len() as u32,
        )?;
        // heat moves through every cell after the matter carrying it has moved
        let fluids: &mut Fluids = self.fluids.as_mut()
            .expect("fluid storage exists with the tile buffers");
        self.thermal.as_mut()
            .expect("thermal simulation exists before fluid conversions")
            .simulate_thermal(
                accelerator,
                active_region,
                cellular_tile_streaming_state.cellular_tile_ring_offset(),
                self.ambient_gas_temperature,
                self.ambient_gas_heat_capacity,
                self.ambient_gas_conduction,
            );
        fluids.simulate_heat(accelerator);
        collision.request_changed_cells(
            accelerator,
            active_region,
            cellular_tile_streaming_state.cellular_tile_ring_offset(),
        );
        if let Some((streaming_request_identifier, _)) = particle_export_target {
            self.cellular_tile_streaming_state.as_mut()
                .expect("tile streaming state exists after simulation")
                .mark_fluid_particle_export_submitted(streaming_request_identifier)?;
        }
        Ok(())
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
        let mut parallax: Vec<SceneGraphicsParallax<'_>> = Vec::new();
        for (actor_sprites, position, actor_parallax) in self.actor_registry
            .components_with_optional::<
                ActorSpritesComponent,
                ActorKinematicsPositionComponent,
                ActorParallaxComponent,
            >()
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
                if let Some(actor_parallax) = actor_parallax {
                    if !actor_parallax.depth.is_finite() || actor_parallax.depth < 0.0
                        || actor_parallax.scroll_factor.iter().chain(actor_parallax.offset.iter())
                            .any(|value| !value.is_finite())
                        || [*offset_x, *offset_y, *scale_x, *scale_y, *rotation]
                            .into_iter().chain(*color).any(|value| !value.is_finite())
                    { continue; }
                    let projection: f64 = 1.0 / (1.0 + f64::from(actor_parallax.depth));
                    let (camera_scroll_x, camera_scroll_y) =
                        camera_position.relative_position(actor_parallax.scroll_origin);
                    // Preserve exact nearby subtraction for the default scroll factors.
                    let scroll_x = camera_scroll_x * (1.0 - f64::from(actor_parallax.scroll_factor[0]));
                    let scroll_y = camera_scroll_y * (1.0 - f64::from(actor_parallax.scroll_factor[1]));
                    // Use the same pixel-to-cell scale as ordinary actor sprites.
                    // Signed scales retain the sprite system's horizontal/vertical flipping.
                    parallax.push(SceneGraphicsParallax {
                        texture: sprite_sheet.texture_view(),
                        center: [
                            ((actor_x + f64::from(*offset_x) + f64::from(actor_parallax.offset[0]) + scroll_x) * projection) as f32,
                            ((actor_y + f64::from(*offset_y) + f64::from(actor_parallax.offset[1]) + scroll_y) * projection) as f32,
                        ],
                        size: [
                            (f64::from(sprite_sheet.width()) * f64::from(*scale_x)
                                / CELLULAR_TILE_SIZE as f64 * projection) as f32,
                            (f64::from(sprite_sheet.height()) * f64::from(*scale_y)
                                / CELLULAR_TILE_SIZE as f64 * projection) as f32,
                        ],
                        depth: actor_parallax.depth,
                        repeat: actor_parallax.repeat,
                        order: *depth,
                        rotation: *rotation,
                        texture_region,
                        color: *color,
                    });
                    continue;
                }
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
            cellular_tile_size: CELLULAR_TILE_SIZE as u32,
            cell_grid_origin: [
                -(camera_position.offset_x as f64 / ScenePosition::OFFSET_UNITS_PER_TILE) as f32,
                -(camera_position.offset_y as f64 / ScenePosition::OFFSET_UNITS_PER_TILE) as f32,
            ],
            cellular: self.cellular_graphics(camera_position),
            fluid: self.fluid_graphics(camera_position),
            gas: self.gas_graphics(camera_position),
            pressure: self.pressure_graphics(camera_position),
            parallax,
            sprites,
            overlays: self.actor_registry
                .components::<ActorOverlayComponent, ActorKinematicsPositionComponent>()
                .map(|(overlay, position)| {
                    let (center_x, center_y): (f64, f64) =
                        position.position.relative_position(camera_position);
                    SceneGraphicsOverlay {
                        center_x: center_x as f32,
                        center_y: center_y as f32,
                        shape: match overlay.shape {
                            ActorOverlayShape::Circle { radius } =>
                                SceneGraphicsOverlayShape::Circle { radius },
                            ActorOverlayShape::Rectangle { width, height } =>
                                SceneGraphicsOverlayShape::Rectangle { width, height },
                        },
                        color: overlay.color,
                    }
                })
                .collect(),
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

    /// Returns the resident fluid raster relative to a camera, or `None` before it streams
    pub fn fluid_graphics(
        &self,
        camera_position: ScenePosition,
    ) -> Option<SceneGraphicsFluid<'_>> {
        let cellular_tile_streaming_state: &CellularTileStreamingState =
            self.cellular_tile_streaming_state.as_ref()?;
        let fluids: &Fluids = self.fluids.as_ref()?;
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
        Some(SceneGraphicsFluid {
            raster_buffer: fluids.fluid_raster_buffer(),
            material_properties_buffer: self.material_table.fluid_properties_buffer(),
            region_x: region_x as f32,
            region_y: region_y as f32,
            region_width: active_region.width() as u32,
            region_height: active_region.height() as u32,
            ring_offset_x,
            ring_offset_y,
            cellular_tile_size: CELLULAR_TILE_SIZE as u32,
        })
    }

    /// Returns the resident gas relative to a camera, or `None` before it streams
    pub fn gas_graphics(
        &self,
        camera_position: ScenePosition,
    ) -> Option<SceneGraphicsGas<'_>> {
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
        Some(SceneGraphicsGas {
            gas_buffer: cellular_tile_buffers.gas_buffer(),
            material_identifier_buffer: cellular_tile_buffers.material_identifier_buffer(),
            material_properties_buffer: self.material_table.gas_properties_buffer(),
            region_x: region_x as f32,
            region_y: region_y as f32,
            region_width: active_region.width() as u32,
            region_height: active_region.height() as u32,
            ring_offset_x,
            ring_offset_y,
            cellular_tile_size: CELLULAR_TILE_SIZE as u32,
            amount_per_atmosphere: GAS_AMOUNT_PER_ATMOSPHERE,
        })
    }

    /// Returns the resident pressure relative to a camera, or `None` before it streams
    pub fn pressure_graphics(
        &self,
        camera_position: ScenePosition,
    ) -> Option<SceneGraphicsPressure<'_>> {
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
        Some(SceneGraphicsPressure {
            pressure_buffer: cellular_tile_buffers.pressure_buffer(),
            ambient_pressure: self.ambient_gas_pressure,
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
        let ambient_gas_amount: u32 = self.ambient_gas_amount();
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
        let tile_result: Result<(), SceneError> = self.edit_streaming_state
            .stream_edits(
                accelerator,
                cellular_tile_buffers,
                cellular_tile_streaming_state,
                &self.actor_registry,
                None,
                ambient_gas_amount,
            )
            .and_then(|()| cellular_tile_streaming_state.return_cellular_tiles(
                accelerator,
                cellular_tile_buffers,
                chunk_streaming_state,
                ambient_gas_amount,
                self.ambient_gas_temperature,
            ));
        if let Err(error) = tile_result {
            panic!("Scene cellular tiles could not be returned: {error:?}");
        }
        let fluid_result: Result<(), SceneError> = (|| {
            let Some(active_region) = cellular_tile_streaming_state
                .active_cellular_tile_region()
            else { return Ok(()); };
            let Some(fluids) = self.fluids.as_mut() else { return Ok(()); };
            let unspawned_particles: Vec<(
                ChunkCoordinates,
                ChunkFluidParticle,
            )> = fluids.drain_unspawned_particles_blocking(accelerator)?;
            chunk_streaming_state.append_fluid_particles(unspawned_particles)?;
            let snapshot_region: CellularTileRegion =
                fluids.particle_snapshot_region(active_region);
            let live_particles: Vec<(
                ChunkCoordinates,
                ChunkFluidParticle,
            )> = fluids.read_live_particles_blocking(accelerator, active_region)?;
            let snapshot_chunks: Vec<ChunkCoordinates> =
                Self::chunk_coordinates_in_region(snapshot_region);
            chunk_streaming_state.replace_resident_fluid_particle_snapshot(
                &snapshot_chunks,
                live_particles,
            )
        })();
        if let Err(error) = fluid_result {
            panic!("Scene fluid particles could not be returned: {error:?}");
        }
    }

}
