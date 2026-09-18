// Copyright Rob Gage 2026

use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::VecDeque;
use std::io;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::SyncSender;
use std::time::Duration;

use engine_compute::Accelerator;
use engine_compute::AcceleratorBuffer;

use super::scene_pending_rigid_dormancy::ScenePendingRigidDormancy;
use super::scene_pending_static_detachment::ScenePendingStaticDetachment;
use super::scene_rigid_body_streaming_response::SceneRigidBodyStreamingResponse;
use super::scene_rigid_cell_removal_cause::SceneRigidCellRemovalCause;
use super::scene_rigid_dormancy_batch::SceneRigidDormancyBatch;
use super::scene_rigid_io_job::SceneRigidIoJob;
use super::scene_rigid_owner_load::SceneRigidOwnerLoad;
use super::scene_rigid_persistence_request::SceneRigidPersistenceRequest;
use crate::actors::Actor;
use crate::actors::ActorContactEvent;
use crate::actors::ActorPhysicalSnapshot;
use crate::actors::ActorRegistry;
use crate::chunks::ChunkEntry;
use crate::chunks::ChunkStreamingResponse;
use crate::materials::MaterialIdentifier;
use crate::materials::MaterialRegistry;
use crate::materials::MaterialTable;
use crate::scenes::FluidDownload;
use crate::scenes::FluidUpload;
use crate::scenes::GasDownload;
use crate::scenes::SceneData;
use crate::scenes::SceneEditBatch;
use crate::scenes::SceneGenerator;
use crate::scenes::ScenePosition;
use crate::scenes::SceneVelocity;
use crate::scenes::TileDownload;
use crate::scenes::TileUpload;
use crate::simulation::CellularCollision;
use crate::simulation::CellularDynamic;
use crate::simulation::CellularPhysicsBodyProxy;
use crate::simulation::CellularPressure;
use crate::simulation::CellularStaticStateGather;
use crate::simulation::CollisionOccupancySnapshot;
use crate::simulation::Fluids;
use crate::simulation::Gases;
use crate::simulation::MaterialMutations;
use crate::simulation::MaterialReactions;
use crate::simulation::RigidCellStateGather;
use crate::simulation::RigidCellStateUpload;
use crate::simulation::RigidCellularBody;
#[cfg(test)]
use crate::simulation::RigidCellularBodyState;
use crate::simulation::ScenePhysicsWorld;
use crate::simulation::ThermalConduction;
use crate::simulation::ThermalEdits;
use crate::simulation::ThermalInteraction;
use crate::simulation::ThermalPhaseTransitions;
use crate::simulation::ThermalScatter;
#[cfg(test)]
use crate::tiles::CellCoordinates;
use crate::tiles::Tile;
use crate::tiles::TileArea;
use crate::tiles::TileCoordinates;

// one serialized owner-file worker avoids lost updates when several bodies
// share an owner; its latency is outside the frame loop.
const RIGID_IO_MAX_IN_FLIGHT: usize = 1;
const RIGID_IO_QUEUE_CAPACITY: usize = 64;
const RIGID_DORMANCY_READBACK_SLOTS: usize = 2;

/// The capacity of the chunk streaming queue
pub const CHUNK_STREAMING_QUEUE_CAPACITY: usize = 64;

/// The fixed scene tick rate
const TICK_RATE: u32 = 60;
const MAX_CATCH_UP_TICKS: u32 = 4;

const RIGID_DETACHMENT_MAXIMUM_CELLS: usize = 1024;

/// A scene that can be simulated by the engine
pub struct Scene {
    /// The `Accelerator` this `Scene` is running on
    accelerator: Arc<Accelerator>,
    /// The persistent `SceneData` backing this `Scene`
    data: SceneData,
    /// The `SceneGenerator` used to generate new tiles for this `Scene`
    generator: Arc<dyn SceneGenerator>,
    /// The `ActorRegistry` currently managed by this `Scene`
    actor_registry: ActorRegistry,
    /// The actor currently receiving player control, if any
    possessed_actor: Option<Actor>,
    pub(super) actor_contact_events: Vec<ActorContactEvent>,
    pub(super) actor_snapshots: HashMap<TileCoordinates, Vec<ActorPhysicalSnapshot>>,
    pub(super) actor_initialized_regions: HashSet<TileCoordinates>,
    /// Chunks in this scene indexed by their `TilePosition`s
    chunks: HashMap<TileCoordinates, ChunkEntry>,
    /// The sender used by chunk streaming threads to return streamed chunks
    chunk_streaming_response_sender: SyncSender<ChunkStreamingResponse>,
    /// The chunks that are being streamed into the `Scene`
    chunk_streaming_responses: Receiver<ChunkStreamingResponse>,
    /// The next identifier to be used for streaming chunks
    chunks_streaming_identifier_next: u64,
    /// Elapsed time not yet consumed by fixed ticks
    tick_time: Duration,
    /// Mandatory world-space edits submitted by editor and gameplay producers.
    pending_runtime_edits: SceneEditBatch,
    /// The width of the active tile area
    simulation_width: u16,
    /// The height of the active tile area
    simulation_height: u16,
    /// The size of the Accelerator tile buffer outside the active area
    simulation_buffer_size: u8,
    /// The `TilePosition` of the tile in `tiles` that is furthest to the left and bottom
    origin: TileCoordinates,
    /// The desired `origin` for the active tile area
    origin_target: TileCoordinates,
    /// An explicit area-follow request to apply before automatic pawn following
    area_request: Option<TileCoordinates>,
    /// The current Accelerator-resident tiles in this `Scene`
    tiles: Box<[Tile]>,
    /// The streaming batch size for tiles
    tile_streaming_batch_size: u8,
    /// The tile downloads pending processing by `tick`
    tile_downloads: Mutex<Vec<Arc<Mutex<TileDownload>>>>,
    /// Mandatory outgoing downloads awaiting application to their CPU chunks
    outgoing_tile_downloads: Vec<Arc<Mutex<TileDownload>>>,
    /// Fluid exports that own particles until asynchronous readback reaches their CPU chunks
    fluid_downloads: Vec<Arc<Mutex<FluidDownload>>>,
    /// Completed fluid-export staging storage available for reuse
    fluid_download_pool: Vec<Arc<Mutex<FluidDownload>>>,
    /// Gas exports that own outgoing fields until asynchronous readback reaches CPU chunks
    gas_downloads: Vec<Arc<Mutex<GasDownload>>>,
    /// Completed gas-export staging storage available for reuse
    gas_download_pool: Vec<Arc<Mutex<GasDownload>>>,
    /// The tile uploads pending processing by `tick`
    tile_uploads: Mutex<Vec<Arc<Mutex<TileUpload>>>>,
    /// Fluid imports that own dormant records until Accelerator reconstruction is confirmed
    fluid_uploads: Vec<Arc<Mutex<FluidUpload>>>,
    /// Fixed staging storage for the possessed pawn's asynchronous derived-fluid sample
    fluid_sample_buffer: wgpu::Buffer,
    /// Completed derived-fluid sample or readback failure
    fluid_sample_result: Arc<Mutex<Option<Result<[f32; 5], String>>>>,
    /// Actor for which the in-flight derived-fluid sample was generated
    fluid_sample_actor: Option<Actor>,
    /// The physical X slot containing the buffered area's leftmost tile
    tiles_ring_offset_x: u16,
    /// The physical Y slot containing the buffered area's bottommost tile
    tiles_ring_offset_y: u16,
    /// The buffer containing `MaterialIdentifier`s for Accelerator-resident tiles
    cellular_material_identifiers: AcceleratorBuffer,
    /// The parallel buffer containing persistent cell appearance samples
    cellular_appearances: AcceleratorBuffer,
    /// The parallel buffer containing persistent static-cell integrity
    cellular_integrities: AcceleratorBuffer,
    cellular_amounts: AcceleratorBuffer,
    cellular_temperatures: AcceleratorBuffer,
    /// Ambient fallback for authored matter without a material temperature default.
    ambient_temperature: f32,
    /// Fixed-capacity persistent integrity for authoritative rigid cells.
    rigid_cell_integrities: AcceleratorBuffer,
    /// Fixed-capacity persistent material inventory for authoritative rigid cells.
    rigid_cell_amounts: AcceleratorBuffer,
    /// Fixed-capacity persistent temperature for authoritative rigid cells.
    rigid_cell_temperatures: AcceleratorBuffer,
    rigid_cell_state_upload: RigidCellStateUpload,
    rigid_cell_state_gather: RigidCellStateGather,
    cellular_static_state_gather: CellularStaticStateGather,
    /// Transient rasterized possessed-pawn interaction geometry
    cellular_physics_body_proxy: CellularPhysicsBodyProxy,
    /// Authoritative body-local cellular matter paired with Rapier bodies
    rigid_cellular_bodies: Vec<RigidCellularBody>,
    /// Monotonic scene identity; never derived from vector or Accelerator allocation order.
    rigid_cellular_body_identifier_next: u64,
    /// Generation protects a delayed raster result from a recycled slot.
    rigid_cell_state_generations: Vec<u32>,
    rigid_cell_state_free: Vec<u32>,
    /// Bodies awaiting one batched authoritative Accelerator state capture.
    rigid_dormancy_batches: Vec<SceneRigidDormancyBatch>,
    rigid_dormancy_readbacks: Vec<wgpu::Buffer>,
    rigid_dormancy_readback_free: Vec<usize>,
    /// Background rigid owner-file work is deliberately bounded independently
    /// from chunk streaming so filesystem latency cannot stall simulation.
    rigid_streaming_response_sender: SyncSender<SceneRigidBodyStreamingResponse>,
    rigid_streaming_responses: Receiver<SceneRigidBodyStreamingResponse>,
    rigid_owner_loads: HashMap<TileCoordinates, SceneRigidOwnerLoad>,
    rigid_owner_load_queue: VecDeque<TileCoordinates>,
    rigid_owner_generation: HashMap<TileCoordinates, u64>,
    rigid_desired_owners: HashSet<TileCoordinates>,
    rigid_persistence_queue: VecDeque<SceneRigidIoJob>,
    rigid_io_in_flight: usize,
    /// Restored bodies wait for a collision snapshot of the current ring.
    rigid_activation_pending: HashSet<u64>,
    rigid_sleeping_pending: HashSet<u64>,
    /// Current-origin terrain has been applied to Rapier after this snapshot.
    rigid_activation_collision_origin: Option<TileCoordinates>,
    /// Changes whenever rigid body-local topology changes
    rigid_cellular_topology_revision: u64,
    /// Last asynchronously confirmed cellular contact state per rigid vector index
    rigid_cellular_contact_active: Vec<bool>,
    rigid_cellular_support: Vec<[f32; 4]>,
    rigid_cellular_recovery: Vec<[f32; 4]>,
    /// Last asynchronously confirmed granular contact state per rigid vector index
    rigid_granular_contact_active: Vec<bool>,
    /// Prior static snapshot used to ignore initial islands and detect topology changes
    rigid_detachment_snapshot: Option<CollisionOccupancySnapshot>,
    pending_static_detachment: Option<ScenePendingStaticDetachment>,
    static_detachment_in_flight_generation: Option<u64>,
    static_detachment_generation: u64,
    static_detachment_visit_stamps: Vec<u32>,
    static_detachment_visit_generation: u32,
    /// Accelerator-authoritative fluid particles and their transient cellular representation
    fluids: Fluids,
    /// Accelerator-authoritative shared gas velocity and per-species concentrations
    gases: Gases,
    /// Accelerator-resident cross-form material replacement requests.
    material_mutations: MaterialMutations,
    thermal_edits: ThermalEdits,
    material_table: MaterialTable,
    material_reactions: MaterialReactions,
    thermal_interaction: ThermalInteraction,
    thermal_conduction: ThermalConduction,
    thermal_scatter: ThermalScatter,
    thermal_phase_transitions: ThermalPhaseTransitions,
    /// Accelerator simulation of dynamic cells in the canonical cellular buffers
    cellular_dynamic: CellularDynamic,
    /// Accelerator impulse, pressure, integrity, and fracture subsystem
    cellular_pressure: CellularPressure,
    /// Compact CPU-readable occupancy derived from the authoritative cellular Accelerator buffer
    cellular_collision: CellularCollision,
    /// Whether cellular data or its ring mapping needs a replacement collision extraction
    cellular_collision_dirty: bool,
    /// Scene gravity acceleration in tiles per second squared
    gravity: [f32; 2],
    /// Rapier rigid-body world plus the CPU-readable actor collision snapshot
    physics_world: ScenePhysicsWorld,
}

#[path = "scene_actor_control.rs"]
mod scene_actor_control;
#[path = "scene_cell_editing.rs"]
mod scene_cell_editing;
#[path = "scene_chunk_navigation.rs"]
mod scene_chunk_navigation;
#[path = "scene_chunk_streaming.rs"]
mod scene_chunk_streaming;
#[path = "scene_construction.rs"]
mod scene_construction;
#[path = "scene_construction_entrypoints.rs"]
mod scene_construction_entrypoints;
#[path = "scene_construction_load.rs"]
mod scene_construction_load;
#[path = "scene_edit_application.rs"]
mod scene_edit_application;
#[path = "scene_fluid_streaming.rs"]
mod scene_fluid_streaming;
#[path = "scene_gas_streaming.rs"]
mod scene_gas_streaming;
#[path = "scene_generator_configuration.rs"]
mod scene_generator_configuration;
#[path = "scene_graphics.rs"]
mod scene_graphics;
#[path = "scene_rigid_cell_mutation.rs"]
mod scene_rigid_cell_mutation;
#[path = "scene_rigid_detachment.rs"]
mod scene_rigid_detachment;
#[path = "scene_rigid_dormancy.rs"]
mod scene_rigid_dormancy;
#[path = "scene_rigid_persistence.rs"]
mod scene_rigid_persistence;
#[path = "scene_tile_access.rs"]
mod scene_tile_access;
#[path = "scene_tile_streaming.rs"]
mod scene_tile_streaming;
#[path = "scene_update.rs"]
mod scene_update;

impl Scene {
    /// Returns the materials registered for this scene
    pub fn materials(&self) -> &MaterialRegistry {
        self.data.materials()
    }

    /// Requests that the active scene area recenter around a world position
    pub fn request_area_around(&mut self, position: ScenePosition) {
        self.area_request = Some(TileCoordinates {
            x: position.tile_coordinates.x - i32::from(self.simulation_width) / 2,
            y: position.tile_coordinates.y - i32::from(self.simulation_height) / 2,
        });
    }

    /// Returns whether a world position is currently resident in the Accelerator tile buffer
    pub fn is_position_resident(&self, position: ScenePosition) -> bool {
        self.area_buffered().contains(position.tile_coordinates)
    }

    /// Returns the authored default temperature or the scene ambient temperature
    fn initial_temperature(&self, material_identifier: MaterialIdentifier) -> f32 {
        self.data
            .materials()
            .thermal_properties(material_identifier)
            .and_then(|properties| properties.default_temperature)
            .unwrap_or(self.ambient_temperature)
    }

    #[cfg(test)]
    pub(crate) const fn rigid_cell_amounts_buffer(&self) -> &AcceleratorBuffer {
        &self.rigid_cell_amounts
    }

    pub(crate) const fn rigid_cell_temperatures_buffer(&self) -> &AcceleratorBuffer {
        &self.rigid_cell_temperatures
    }

    /// Removes completed Accelerator tile uploads
    fn tile_uploads_clean(&self) -> Result<(), io::Error> {
        self.tile_uploads
            .lock()
            .map_err(|_| io::Error::other("Tile upload queue is unavailable"))?
            .retain(|upload| !upload.lock().unwrap().is_complete);
        Ok(())
    }

    #[cfg(test)]
    pub(crate) const fn test_cellular_material_identifiers_buffer(&self) -> &AcceleratorBuffer {
        &self.cellular_material_identifiers
    }

    #[cfg(test)]
    pub(crate) const fn test_cellular_amounts_buffer(&self) -> &AcceleratorBuffer {
        &self.cellular_amounts
    }

    #[cfg(test)]
    pub(crate) const fn test_fluid_particles_buffer(&self) -> &AcceleratorBuffer {
        self.fluids.particles_buffer()
    }

    #[cfg(test)]
    pub(crate) fn test_apply_edits_immediate(
        &mut self,
        edits: &mut SceneEditBatch,
    ) -> Result<(), io::Error> {
        self.apply_edits_immediate(edits)
    }

    #[cfg(test)]
    pub(crate) fn test_shift_to(&mut self, origin: TileCoordinates) -> Result<(), io::Error> {
        self.shift_to(origin)
    }

    #[cfg(test)]
    pub(crate) fn test_set_origin_target_to_origin(&mut self) {
        self.origin_target = self.origin;
    }

    #[cfg(test)]
    pub(crate) fn test_has_pending_streaming_downloads(&self) -> bool {
        !self.gas_downloads.is_empty()
            || !self.fluid_downloads.is_empty()
            || !self.outgoing_tile_downloads.is_empty()
    }

    #[cfg(test)]
    pub(crate) const fn test_gas_count(&self) -> u32 {
        self.gases.gas_count()
    }

    #[cfg(test)]
    pub(crate) fn test_export_gases(&self, accelerator: &Accelerator, download: &GasDownload) {
        let buffered_area: TileArea = self.area_buffered();
        let dimensions: [u16; 2] = buffered_area.dimensions();
        self.gases.export(
            accelerator,
            download,
            buffered_area.origin(),
            dimensions[0],
            dimensions[1],
            self.tiles_ring_offset_x,
            self.tiles_ring_offset_y,
        );
    }

    #[cfg(test)]
    pub(crate) fn test_commit_reserved_particle(
        &self,
        accelerator: &Accelerator,
        slot: u32,
        material: u32,
        position: [f32; 2],
        velocity: [f32; 2],
        amount: f32,
        temperature: f32,
    ) {
        self.fluids.commit_reserved_particle(
            accelerator,
            slot,
            material,
            position,
            velocity,
            amount,
            temperature,
        );
    }

    #[cfg(test)]
    pub(crate) const fn test_particle_capacity(&self) -> u32 {
        self.fluids.particle_capacity()
    }

    #[cfg(test)]
    pub(crate) fn test_rigid_cellular_bodies(&self) -> &[RigidCellularBody] {
        &self.rigid_cellular_bodies
    }

    #[cfg(test)]
    pub(crate) fn test_apply_completed_static_detachment(&mut self) -> Result<(), io::Error> {
        self.apply_completed_static_detachment()
    }

    #[cfg(test)]
    pub(crate) fn test_detach_unanchored_static_components(
        &mut self,
        snapshot: &mut CollisionOccupancySnapshot,
    ) -> Result<(), io::Error> {
        self.detach_unanchored_static_components(snapshot)
    }

    #[cfg(test)]
    pub(crate) fn test_cell_edit_index(&self, coordinates: CellCoordinates) -> Option<usize> {
        self.cell_edit_index(coordinates)
    }

    #[cfg(test)]
    pub(crate) fn test_physics_world(&mut self) -> &mut ScenePhysicsWorld {
        &mut self.physics_world
    }

    #[cfg(test)]
    pub(crate) const fn test_gravity(&self) -> [f32; 2] {
        self.gravity
    }

    #[cfg(test)]
    pub(crate) const fn test_rigid_cellular_topology_revision(&self) -> u64 {
        self.rigid_cellular_topology_revision
    }

    #[cfg(test)]
    pub(crate) fn test_rigid_cellular_body_state(&self) -> Option<RigidCellularBodyState> {
        self.rigid_cellular_bodies
            .first()
            .and_then(|body| self.physics_world.rigid_cellular_body_state(body))
    }

    #[cfg(test)]
    pub(crate) fn test_rasterize_rigid_cellular_bodies(
        &mut self,
        accelerator: &Accelerator,
        state: RigidCellularBodyState,
    ) {
        self.cellular_physics_body_proxy.rasterize(
            accelerator,
            TileCoordinates { x: -2, y: -2 },
            5,
            5,
            0,
            0,
            self.gravity,
            &[],
            &self.rigid_cellular_bodies,
            &[state],
            self.rigid_cellular_topology_revision,
        );
    }
}

impl Drop for Scene {
    fn drop(&mut self) {
        self.cellular_material_identifiers.free();
        self.cellular_appearances.free();
        self.cellular_integrities.free();
        self.rigid_cell_integrities.free();
        self.rigid_cell_amounts.free();
        self.rigid_cell_temperatures.free();
        self.fluid_sample_buffer.destroy();
    }
}
