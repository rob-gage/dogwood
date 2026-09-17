// Copyright Rob Gage 2026

use super::scene_pending_rigid_dormancy::PendingRigidDormancy;
use super::scene_pending_static_detachment::PendingStaticDetachment;
use super::scene_rigid_cell_removal_cause::RigidCellRemovalCause;
use super::scene_rigid_dormancy_batch::RigidDormancyBatch;
use super::scene_rigid_io_job::RigidIoJob;
use super::scene_rigid_owner_load::RigidOwnerLoad;
use super::scene_rigid_persistence_request::RigidPersistenceRequest;
use super::scene_rigid_streaming_response::RigidStreamingResponse;
use crate::scenes::{
    FluidDownload, FluidUpload, GasDownload, GasUpload, SceneData, SceneEdit, SceneEditBatch,
    SceneEditCellPlacement, SceneGenerator, ScenePosition, SceneVelocity, TileDownload, TileUpload,
};
use crate::simulation::{
    CellularCollision, CellularDynamic, CellularPhysicsBodyProxy, CellularPressure,
    CellularStaticStateGather, CollisionOccupancySnapshot, Fluids, Gases, MaterialMutations,
    MaterialReactions, RigidCellStateGather, RigidCellStateUpload, RigidCellularBody,
    RigidCellularBodyCell, RigidCellularBodyState, ScenePhysicsWorld, SceneSimulationConfiguration,
    ThermalConduction, ThermalEdits, ThermalInteraction, ThermalPhaseTransitions, ThermalScatter,
};
use crate::{
    actors::{Actor, ActorRegistry},
    chunks::{Chunk, ChunkEntry, ChunkFluidParticle, ChunkGasCell, ChunkStreamingResponse},
    materials::{Material, MaterialIdentifier, MaterialRegistry, MaterialTable},
    tiles::{CellCoordinates, CellularAppearance, Tile, TileArea, TileCoordinates, TileData},
};
use engine_compute::{Accelerator, AcceleratorBuffer};
use engine_graphics::SceneGraphics;
use rapier2d::prelude::Vector;
use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    error::Error,
    future::poll_fn,
    io,
    sync::{
        Arc, Mutex,
        mpsc::{Receiver, SyncSender, sync_channel},
    },
    time::Duration,
};

// One serialized owner-file worker avoids lost updates when several bodies
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
    rigid_cellular_body_id_next: u64,
    /// Generation protects a delayed raster result from a recycled slot.
    rigid_cell_state_generations: Vec<u32>,
    rigid_cell_state_free: Vec<u32>,
    /// Bodies awaiting one batched authoritative Accelerator state capture.
    rigid_dormancy_batches: Vec<RigidDormancyBatch>,
    rigid_dormancy_readbacks: Vec<wgpu::Buffer>,
    rigid_dormancy_readback_free: Vec<usize>,
    /// Background rigid owner-file work is deliberately bounded independently
    /// from chunk streaming so filesystem latency cannot stall simulation.
    rigid_streaming_response_sender: SyncSender<RigidStreamingResponse>,
    rigid_streaming_responses: Receiver<RigidStreamingResponse>,
    rigid_owner_loads: HashMap<TileCoordinates, RigidOwnerLoad>,
    rigid_owner_load_queue: VecDeque<TileCoordinates>,
    rigid_owner_generation: HashMap<TileCoordinates, u64>,
    rigid_desired_owners: HashSet<TileCoordinates>,
    rigid_persistence_queue: VecDeque<RigidIoJob>,
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
    pending_static_detachment: Option<PendingStaticDetachment>,
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

#[path = "scene_construction.rs"]
mod scene_construction;

impl Scene {
    /// Returns graphics information for this scene
    pub fn graphics(&self) -> SceneGraphics<'_> {
        let buffer_size: i32 = i32::from(self.simulation_buffer_size);
        let dimensions: u32 = u32::from(self.simulation_buffer_size) * 2;
        let walking_pawn: Option<([f32; 2], [f32; 2])> = self
            .actor_registry
            .first_walking_pawn_graphics(self.tick_interpolation());
        SceneGraphics {
            material_graphics: self.material_table.graphics(),
            cellular_material_identifiers: &self.cellular_material_identifiers,
            cellular_appearances: &self.cellular_appearances,
            rigid_material_identifiers: self
                .cellular_physics_body_proxy
                .rigid_material_identifiers_buffer(),
            rigid_appearances: self.cellular_physics_body_proxy.rigid_appearances_buffer(),
            fluid_material_identifiers: self.fluids.material_identifiers_buffer(),
            fluid_coverage: self.fluids.coverage_buffer(),
            gas_concentrations: self.gases.concentrations_buffer(),
            gas_count: self.gases.gas_count(),
            cellular_pressure: self.cellular_pressure.retained_pressure(),
            buffered_origin: [self.origin.x - buffer_size, self.origin.y - buffer_size],
            buffered_tile_size: [
                u32::from(self.simulation_width) + dimensions,
                u32::from(self.simulation_height) + dimensions,
            ],
            ring_offset: [
                u32::from(self.tiles_ring_offset_x),
                u32::from(self.tiles_ring_offset_y),
            ],
            walking_pawn,
        }
    }

    /// Returns the materials registered for this scene
    pub fn materials(&self) -> &MaterialRegistry {
        self.data.materials()
    }

    /// Returns the `ActorRegistry` for this `Scene`
    pub const fn actor_registry(&self) -> &ActorRegistry {
        &self.actor_registry
    }

    /// Returns mutable access to the `ActorRegistry` for this `Scene`
    pub const fn actor_registry_mutable(&mut self) -> &mut ActorRegistry {
        &mut self.actor_registry
    }

    /// Returns the currently possessed actor if one exists
    pub fn possessed_actor(&self) -> Option<Actor> {
        self.possessed_actor
            .filter(|actor| self.actor_registry.contains(*actor))
    }

    /// Returns an actor's position interpolated between its latest fixed ticks
    pub fn actor_render_position(&self, actor: Actor) -> Option<ScenePosition> {
        self.actor_registry
            .get_render_position(actor, self.tick_interpolation())
    }

    /// Possesses an actor if it exists in this `Scene`
    pub fn possess_actor(&mut self, identifier: Actor) -> bool {
        if !self.actor_registry.is_possessable(identifier) {
            return false;
        }
        if self.possessed_actor != Some(identifier)
            && let Some(possessed) = self.possessed_actor
        {
            self.actor_registry.clear_control_state(possessed);
        }
        self.possessed_actor = Some(identifier);
        true
    }

    /// Releases the currently possessed actor
    pub fn dispossess_actor(&mut self) {
        if let Some(possessed) = self.possessed_actor.take() {
            self.actor_registry.clear_control_state(possessed);
        }
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

    /// Queues mandatory runtime material edits for one coalesced update-time flush.
    pub fn queue_edits(&mut self, edits: SceneEditBatch) {
        self.pending_runtime_edits.append(edits);
    }

    /// Applies a scene-owned transaction immediately; nonresident requests remain queued.
    fn apply_edits_immediate(&mut self, edits: &mut SceneEditBatch) -> Result<(), io::Error> {
        let mut cell_edits: HashMap<
            usize,
            (CellCoordinates, MaterialIdentifier, CellularAppearance, f32),
        > = HashMap::new();
        let mut fluid_edits: HashMap<usize, u32> = HashMap::new();
        let mut gas_edits: BTreeMap<(usize, u32), f32> = BTreeMap::new();
        let mut gas_clear_cells: HashSet<usize> = HashSet::new();
        let mut thermal_edits: BTreeMap<usize, f32> = BTreeMap::new();
        let mut rigid_destroy_indices = Vec::new();
        let mut deferred = SceneEditBatch::new();
        for edit in edits.drain() {
            match edit {
                SceneEdit::PlaceRigidBody { cells } => {
                    if cells.is_empty() {
                        continue;
                    }
                    if !cells
                        .iter()
                        .all(|cell| self.cell_edit_index(cell.coordinates).is_some())
                    {
                        deferred.place_rigid_body(cells);
                    } else {
                        self.insert_authored_rigid_cellular_body(cells);
                    }
                }
                SceneEdit::PlaceCells { cells } => {
                    for SceneEditCellPlacement {
                        coordinates,
                        material_identifier,
                        appearance,
                    } in cells
                    {
                        if let Some(physical_index) = self.cell_edit_index(coordinates) {
                            match self.data.materials().get(material_identifier) {
                                Some(Material::CellularStatic {
                                    default_integrity, ..
                                }) => {
                                    cell_edits.insert(
                                        physical_index,
                                        (
                                            coordinates,
                                            material_identifier,
                                            appearance,
                                            *default_integrity,
                                        ),
                                    );
                                    fluid_edits.insert(physical_index, Fluids::erase_edit());
                                    if self.gases.gas_count() != 0 {
                                        gas_clear_cells.insert(physical_index);
                                        for species in 0..self.gases.gas_count() {
                                            gas_edits.remove(&(physical_index, species));
                                        }
                                    }
                                }
                                Some(Material::CellularDynamic { .. }) => {
                                    cell_edits.insert(
                                        physical_index,
                                        (coordinates, material_identifier, appearance, 0.0),
                                    );
                                    fluid_edits.insert(physical_index, Fluids::erase_edit());
                                    if self.gases.gas_count() != 0 {
                                        gas_clear_cells.insert(physical_index);
                                        for species in 0..self.gases.gas_count() {
                                            gas_edits.remove(&(physical_index, species));
                                        }
                                    }
                                }
                                Some(Material::Fluid { .. }) => {
                                    cell_edits.insert(
                                        physical_index,
                                        (
                                            coordinates,
                                            MaterialIdentifier::NULL,
                                            CellularAppearance::NEUTRAL,
                                            0.0,
                                        ),
                                    );
                                    fluid_edits
                                        .insert(physical_index, material_identifier.as_u32());
                                    if self.gases.gas_count() != 0 {
                                        gas_clear_cells.insert(physical_index);
                                        for species in 0..self.gases.gas_count() {
                                            gas_edits.remove(&(physical_index, species));
                                        }
                                    }
                                }
                                Some(Material::Gas { .. }) => {
                                    gas_edits.insert(
                                        (physical_index, material_identifier.index()),
                                        self.initial_temperature(material_identifier),
                                    );
                                }
                                None => {}
                            }
                        } else {
                            deferred.place_cells(vec![SceneEditCellPlacement {
                                coordinates,
                                material_identifier,
                                appearance,
                            }]);
                        }
                    }
                }
                SceneEdit::Erase { cells } => {
                    for coordinates in cells {
                        if let Some(physical_index) = self.cell_edit_index(coordinates) {
                            rigid_destroy_indices.push(physical_index);
                            cell_edits.insert(
                                physical_index,
                                (
                                    coordinates,
                                    MaterialIdentifier::NULL,
                                    CellularAppearance::NEUTRAL,
                                    0.0,
                                ),
                            );
                            fluid_edits.insert(physical_index, Fluids::erase_edit());
                            if self.gases.gas_count() != 0 {
                                gas_clear_cells.insert(physical_index);
                                for species in 0..self.gases.gas_count() {
                                    gas_edits.remove(&(physical_index, species));
                                }
                            }
                        } else {
                            deferred.erase(vec![coordinates]);
                        }
                    }
                }
                SceneEdit::DestroyCells { cells } => {
                    for coordinates in cells {
                        if let Some(physical_index) = self.cell_edit_index(coordinates) {
                            rigid_destroy_indices.push(physical_index);
                            cell_edits.insert(
                                physical_index,
                                (
                                    coordinates,
                                    MaterialIdentifier::NULL,
                                    CellularAppearance::NEUTRAL,
                                    0.0,
                                ),
                            );
                            fluid_edits.insert(physical_index, Fluids::erase_edit());
                            if self.gases.gas_count() != 0 {
                                gas_clear_cells.insert(physical_index);
                                for species in 0..self.gases.gas_count() {
                                    gas_edits.remove(&(physical_index, species));
                                }
                            }
                        } else {
                            deferred.destroy_cells(vec![coordinates]);
                        }
                    }
                }
                SceneEdit::Thermal {
                    cells,
                    delta_temperature,
                } => {
                    for coordinates in cells {
                        if let Some(index) = self.cell_edit_index(coordinates) {
                            *thermal_edits.entry(index).or_default() += delta_temperature;
                        } else {
                            deferred.thermal(vec![coordinates], delta_temperature);
                        }
                    }
                }
            }
        }
        edits.append(deferred);
        if !thermal_edits.is_empty() {
            let physical_indices: Vec<u32> =
                thermal_edits.keys().map(|&index| index as u32).collect();
            // The current Accelerator request format has one delta per request; aggregate same-cell
            // edits on the CPU so this flush submits exactly once.
            self.thermal_edits.apply(
                self.accelerator.as_ref(),
                &physical_indices,
                &thermal_edits,
                [
                    self.origin.x - i32::from(self.simulation_buffer_size),
                    self.origin.y - i32::from(self.simulation_buffer_size),
                ],
                [
                    u32::from(self.simulation_width + u16::from(self.simulation_buffer_size) * 2),
                    u32::from(self.simulation_height + u16::from(self.simulation_buffer_size) * 2),
                ],
                [
                    u32::from(self.tiles_ring_offset_x),
                    u32::from(self.tiles_ring_offset_y),
                ],
            );
        }
        if !rigid_destroy_indices.is_empty() {
            rigid_destroy_indices.sort_unstable();
            rigid_destroy_indices.dedup();
            self.cellular_physics_body_proxy
                .resolve_destruction_requests(self.accelerator.as_ref(), &rigid_destroy_indices);
        }
        let mut cell_edits: Vec<(
            usize,
            CellCoordinates,
            MaterialIdentifier,
            CellularAppearance,
            f32,
        )> = cell_edits
            .into_iter()
            .map(
                |(index, (coordinates, material_identifier, appearance, integrity))| {
                    (
                        index,
                        coordinates,
                        material_identifier,
                        appearance,
                        integrity,
                    )
                },
            )
            .collect();
        cell_edits.sort_unstable_by_key(|(physical_index, ..)| *physical_index);
        for (_, coordinates, material_identifier, appearance, integrity) in &cell_edits {
            let tile_coordinates: TileCoordinates = coordinates.tile_coordinates();
            let [x, y]: [usize; 2] = coordinates.local_tile_coordinates();
            let Some(ChunkEntry::Active { chunk, is_dirty }) =
                self.chunks.get_mut(&tile_coordinates.chunk_coordinates())
            else {
                return Err(io::Error::other("Resident tile chunk is not active"));
            };
            chunk
                .set_cell_with_integrity(
                    tile_coordinates,
                    x,
                    y,
                    *material_identifier,
                    *appearance,
                    *integrity,
                )
                .map_err(|_| io::Error::other("Resident tile is not in its active chunk"))?;
            *is_dirty = true;
        }
        if !cell_edits.is_empty() {
            self.cellular_collision_dirty = true;
            self.write_cell_edits(&cell_edits);
        }
        if !fluid_edits.is_empty() {
            let mut fluid_edits: Vec<(usize, u32, f32, f32)> = fluid_edits
                .into_iter()
                .map(|(index, material)| {
                    let identifier = MaterialIdentifier::from_u32(material);
                    let temperature = if identifier == MaterialIdentifier::NULL {
                        0.0
                    } else {
                        self.initial_temperature(identifier)
                    };
                    (index, material, 1.0, temperature)
                })
                .collect();
            fluid_edits.sort_unstable_by_key(|(physical_index, ..)| *physical_index);
            let buffer_size: i32 = i32::from(self.simulation_buffer_size);
            let dimensions: u16 = u16::from(self.simulation_buffer_size) * 2;
            let fluid_active_area: TileArea = self.area_fluid_active();
            let fluid_active_dimensions: [u16; 2] = fluid_active_area.dimensions();
            self.fluids.apply_edits(
                self.accelerator.as_ref(),
                &fluid_edits,
                fluid_active_area.origin(),
                fluid_active_dimensions[0],
                fluid_active_dimensions[1],
                TileCoordinates {
                    x: self.origin.x - buffer_size,
                    y: self.origin.y - buffer_size,
                },
                self.simulation_width + dimensions,
                self.simulation_height + dimensions,
                self.tiles_ring_offset_x,
                self.tiles_ring_offset_y,
            );
        }
        if !gas_edits.is_empty() || !gas_clear_cells.is_empty() {
            let mut authored_temperature_by_cell = BTreeMap::<usize, (f32, usize)>::new();
            // BTreeMap ordering plus this average keeps shared-cell gas temperature deterministic.
            for (&(cell, _), &temperature) in &gas_edits {
                let entry = authored_temperature_by_cell.entry(cell).or_insert((0.0, 0));
                entry.0 += temperature;
                entry.1 += 1;
            }
            let gas_edits: Vec<(usize, u32, f32)> = gas_edits
                .into_iter()
                .map(|((cell, species), _)| {
                    let (sum, count) = authored_temperature_by_cell[&cell];
                    (cell, species, sum / count as f32)
                })
                .collect();
            let mut gas_clear_cells: Vec<usize> = gas_clear_cells.into_iter().collect();
            gas_clear_cells.sort_unstable();
            self.gases
                .apply_edits(self.accelerator.as_ref(), &gas_edits, &gas_clear_cells);
        }
        Ok(())
    }

    /// Applies one Accelerator radial cellular impulse without moving cells immediately.
    pub fn apply_cellular_radial_impulse(
        &mut self,
        center: CellCoordinates,
        radius_cells: f32,
        strength: f32,
    ) {
        let buffer_size: i32 = i32::from(self.simulation_buffer_size);
        let dimensions: u16 = u16::from(self.simulation_buffer_size) * 2;
        self.cellular_pressure.apply_radial_impulse(
            self.accelerator.as_ref(),
            TileCoordinates {
                x: self.origin.x - buffer_size,
                y: self.origin.y - buffer_size,
            },
            self.simulation_width + dimensions,
            self.simulation_height + dimensions,
            self.tiles_ring_offset_x,
            self.tiles_ring_offset_y,
            center,
            radius_cells.max(0.75),
            strength,
        );
    }

    /// Handles Scene streaming and returns the number of completed fixed-rate ticks
    pub fn update(
        &mut self,
        elapsed: Duration,
        is_simulation_active: bool,
    ) -> Result<u32, io::Error> {
        if let Some(origin_target) = self.area_request.take() {
            self.origin_target = origin_target;
        } else {
            let possessed_position: Option<ScenePosition> = self
                .possessed_actor()
                .and_then(|actor| self.actor_registry.get_position(actor))
                .copied();
            if let Some(position) = possessed_position {
                self.follow_position(position);
            }
        }
        self.rigid_streaming_apply_completed()?;
        self.chunks_refresh()?;
        self.tile_downloads_submit()?;
        self.tile_uploads_submit()?;
        self.fluid_downloads_submit()?;
        self.fluid_uploads_submit()?;
        self.gas_downloads_submit()?;
        self.restore_ready_rigids()?;
        if !self.pending_runtime_edits.is_empty() {
            let mut edits = SceneEditBatch::new();
            std::mem::swap(&mut edits, &mut self.pending_runtime_edits);
            self.apply_edits_immediate(&mut edits)?;
            self.pending_runtime_edits.append(edits);
        }
        self.accelerator
            .poll()
            .map_err(|error| io::Error::other(error.to_string()))?;
        self.apply_completed_rigid_thermal_transitions();
        self.apply_completed_rigid_cellular_reactions()?;
        self.rigid_dormancy_apply_completed()?;
        self.cellular_collision.collect_collision()?;
        self.tile_downloads_apply_completed()?;
        self.fluid_downloads_apply_completed()?;
        self.fluid_uploads_apply_completed()?;
        self.gas_downloads_apply_completed()?;
        self.fluid_sample_apply_completed()?;
        self.tile_downloads_clean()?;
        self.tile_uploads_clean()?;
        self.tick_time += elapsed;
        let tick_time: Duration = Duration::from_secs(1) / TICK_RATE;
        self.tick_time = self
            .tick_time
            .min(tick_time.saturating_mul(MAX_CATCH_UP_TICKS));
        let mut ticks: u32 = 0;
        while self.tick_time >= tick_time && ticks < MAX_CATCH_UP_TICKS {
            // A catch-up update may submit several fixed ticks. Give tiny reaction
            // readbacks a nonblocking chance to complete between them so each
            // reaction is applied as its own fixed-tick batch.
            self.accelerator
                .poll()
                .map_err(|error| io::Error::other(error.to_string()))?;
            self.apply_completed_rigid_cellular_reactions()?;
            self.apply_completed_static_detachment()?;
            self.tick(is_simulation_active)?;
            self.tick_time -= tick_time;
            ticks = ticks.saturating_add(1);
        }
        if ticks == MAX_CATCH_UP_TICKS && self.tick_time >= tick_time {
            self.tick_time = tick_time.saturating_sub(Duration::from_nanos(1));
        }
        Ok(ticks)
    }

    /// Returns progress from the previous fixed tick to the current fixed tick
    fn tick_interpolation(&self) -> f32 {
        (self.tick_time.as_secs_f32() * TICK_RATE as f32).clamp(0.0, 1.0)
    }

    /// Applies every compatible completed Accelerator reaction in submission order
    fn apply_completed_rigid_cellular_reactions(&mut self) -> Result<(), io::Error> {
        let mut chemistry_removals: HashMap<usize, HashSet<[i32; 2]>> = HashMap::new();
        for event in self
            .material_reactions
            .take_rigid_removal_events(self.accelerator.as_ref())
            .into_iter()
            .flatten()
        {
            let body = event[3] as usize;
            let local = [event[4] as i32, event[5] as i32];
            if self
                .rigid_cell_state_generations
                .get(event[0] as usize)
                .copied()
                != Some(event[1])
                || self.rigid_cellular_bodies.get(body).is_none_or(|body| {
                    !body.cells.iter().any(|cell| {
                        cell.state_slot == event[0]
                            && cell.state_generation == event[1]
                            && cell.material.as_u32() == event[2]
                            && cell.local == local
                    })
                })
            {
                continue;
            }
            chemistry_removals.entry(body).or_default().insert(local);
        }
        let mut chemistry_removals: Vec<_> = chemistry_removals.into_iter().collect();
        chemistry_removals.sort_unstable_by_key(|(body, _)| std::cmp::Reverse(*body));
        for (body, cells) in chemistry_removals {
            self.remove_rigid_cellular_body_cells(body, &cells, RigidCellRemovalCause::Chemistry);
        }
        let destroyed: HashSet<[u32; 2]> = self
            .cellular_physics_body_proxy
            .take_destroyed_handles()
            .into_iter()
            .filter(|handle| handle[0] != u32::MAX)
            .collect();
        if !destroyed.is_empty() {
            let mut removals: Vec<(usize, HashSet<[i32; 2]>)> = self
                .rigid_cellular_bodies
                .iter()
                .enumerate()
                .filter_map(|(body, rigid)| {
                    let cells = rigid
                        .cells
                        .iter()
                        .filter(|cell| {
                            destroyed.contains(&[cell.state_slot, cell.state_generation])
                        })
                        .map(|cell| cell.local)
                        .collect::<HashSet<_>>();
                    (!cells.is_empty()).then_some((body, cells))
                })
                .collect();
            removals.sort_unstable_by_key(|(body, _)| std::cmp::Reverse(*body));
            for (body, cells) in removals {
                self.remove_rigid_cellular_body_cells(body, &cells, RigidCellRemovalCause::Erase);
            }
        }
        self.debug_assert_rigid_resident_invariants();
        let body_count: usize = self.rigid_cellular_bodies.len();
        let mut newest = None;
        for batch in self.cellular_pressure.collect_rigid_reactions()? {
            if batch.topology_revision != self.rigid_cellular_topology_revision
                || batch.body_count != body_count
            {
                continue;
            }
            for index in 0..batch.body_count {
                let reaction = batch.reactions[index];
                let granular_contacts = batch.granular_contact_counts[index] != 0;
                let contacts = granular_contacts || batch.contact_counts[index] != 0;
                let wake = contacts != self.rigid_cellular_contact_active[index]
                    || batch.moving_contact_counts[index] != 0;
                if !self.physics_world.apply_rigid_cellular_body_reaction(
                    &self.rigid_cellular_bodies[index],
                    [reaction[0], reaction[1]],
                    reaction[2],
                    batch.energy_budgets[index],
                    wake,
                ) {
                    return Err(io::Error::other("Rigid cellular body handle is missing"));
                }
            }
            newest = Some(batch);
        }
        if let Some(batch) = newest {
            for index in 0..body_count {
                let contacts = batch.contact_counts[index] != 0;
                let wake = contacts != self.rigid_cellular_contact_active[index]
                    || batch.moving_contact_counts[index] != 0;
                self.physics_world.apply_rigid_constraint(
                    &self.rigid_cellular_bodies[index],
                    batch.constraints[index],
                    batch.source_motion[index],
                    wake,
                );
                self.rigid_cellular_contact_active[index] = contacts;
                self.rigid_granular_contact_active[index] =
                    batch.granular_contact_counts[index] != 0;
            }
            if !batch.fractured_slots.is_empty() {
                let fractured: HashSet<u32> = batch.fractured_slots.iter().copied().collect();
                let mut removals: Vec<(usize, HashSet<[i32; 2]>)> = self
                    .rigid_cellular_bodies
                    .iter()
                    .enumerate()
                    .filter_map(|(body, rigid)| {
                        let cells: HashSet<[i32; 2]> = rigid
                            .cells
                            .iter()
                            .filter(|cell| fractured.contains(&cell.state_slot))
                            .map(|cell| cell.local)
                            .collect();
                        (!cells.is_empty()).then_some((body, cells))
                    })
                    .collect();
                removals.sort_unstable_by_key(|(body, _)| std::cmp::Reverse(*body));
                for (body, cells) in removals {
                    self.remove_rigid_cellular_body_cells(
                        body,
                        &cells,
                        RigidCellRemovalCause::Fracture,
                    );
                }
            }
        }
        Ok(())
    }

    /// Freezes outgoing bodies while current support is still valid, then
    /// gathers every selected cell through one compact Accelerator batch.
    fn rigid_dormancy_begin(&mut self, future_buffered: TileArea) -> Result<bool, io::Error> {
        let current_buffered = self.area_buffered();
        let mut selected = Vec::new();
        let mut state_count = 0usize;
        for (index, body) in self.rigid_cellular_bodies.iter().enumerate() {
            let Some(state) = self.physics_world.rigid_cellular_body_state(body) else {
                continue;
            };
            let Some(bounds) = crate::scenes::world_aabb(
                state.translation,
                state.angle,
                body.cells.iter().map(|cell| cell.local),
            ) else {
                continue;
            };
            if !body.cells.is_empty()
                && crate::scenes::intersects_area(bounds, current_buffered)
                && !crate::scenes::intersects_area(bounds, future_buffered)
            {
                state_count += body.cells.len();
                selected.push((
                    index,
                    PendingRigidDormancy {
                        id: body.id,
                        position: state.translation,
                        rotation: state.angle,
                        linear_velocity: state.linear_velocity,
                        angular_velocity: state.angular_velocity,
                        sleeping: state.sleeping,
                        cells: body.cells.clone(),
                    },
                ));
            }
        }
        if selected.is_empty() {
            return Ok(true);
        }
        if self.rigid_persistence_queue.len() + self.rigid_io_in_flight + selected.len()
            > RIGID_IO_QUEUE_CAPACITY
        {
            return Ok(false);
        }
        let Some(readback_slot) = self.rigid_dormancy_readback_free.pop() else {
            return Ok(false);
        };
        let slots: Vec<u32> = selected
            .iter()
            .flat_map(|(_, body)| body.cells.iter().map(|cell| cell.state_slot))
            .collect();
        self.rigid_cell_state_gather.submit(
            self.accelerator.as_ref(),
            &slots,
            &self.rigid_dormancy_readbacks[readback_slot],
        );
        let (sender, result) = sync_channel(1);
        self.rigid_dormancy_readbacks[readback_slot]
            .slice(0..state_count as u64 * 16)
            .map_async(wgpu::MapMode::Read, move |outcome| {
                let _ = sender.send(outcome);
            });
        let mut bodies: Vec<_> = selected
            .into_iter()
            .rev()
            .map(|(index, pending)| {
                let body = self.rigid_cellular_bodies.swap_remove(index);
                self.rigid_activation_pending.remove(&body.id);
                self.rigid_sleeping_pending.remove(&body.id);
                self.physics_world.remove_rigid_cellular_body(&body);
                pending
            })
            .collect();
        // Slots were packed in ascending resident order; restore that output
        // order after the descending swap-removes kept indices stable.
        bodies.reverse();
        self.rigid_cellular_topology_revision =
            self.rigid_cellular_topology_revision.wrapping_add(1);
        self.rigid_cellular_contact_active
            .resize(self.rigid_cellular_bodies.len(), false);
        self.rigid_granular_contact_active
            .resize(self.rigid_cellular_bodies.len(), false);
        self.rigid_dormancy_batches.push(RigidDormancyBatch {
            bodies,
            readback_slot,
            state_count,
            result,
        });
        self.debug_assert_rigid_resident_invariants();
        Ok(true)
    }

    fn rigid_dormancy_apply_completed(&mut self) -> Result<(), io::Error> {
        let mut index = 0;
        while index < self.rigid_dormancy_batches.len() {
            match self.rigid_dormancy_batches[index].result.try_recv() {
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    index += 1;
                    continue;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    let batch = self.rigid_dormancy_batches.swap_remove(index);
                    self.rigid_dormancy_readbacks[batch.readback_slot].unmap();
                    self.rigid_dormancy_readback_free.push(batch.readback_slot);
                    self.restore_aborted_rigid_dormancy(batch.bodies);
                    return Err(io::Error::other("rigid dormancy readback disconnected"));
                }
                Ok(Err(error)) => {
                    let batch = self.rigid_dormancy_batches.swap_remove(index);
                    self.rigid_dormancy_readbacks[batch.readback_slot].unmap();
                    self.rigid_dormancy_readback_free.push(batch.readback_slot);
                    self.restore_aborted_rigid_dormancy(batch.bodies);
                    return Err(io::Error::other(format!(
                        "rigid dormancy readback failed: {error}"
                    )));
                }
                Ok(Ok(())) => {}
            }
            let batch = self.rigid_dormancy_batches.swap_remove(index);
            let bytes = match self.rigid_dormancy_readbacks[batch.readback_slot]
                .slice(0..batch.state_count as u64 * 16)
                .get_mapped_range()
            {
                Ok(bytes) => bytes,
                Err(error) => {
                    self.rigid_dormancy_readbacks[batch.readback_slot].unmap();
                    self.rigid_dormancy_readback_free.push(batch.readback_slot);
                    self.restore_aborted_rigid_dormancy(batch.bodies);
                    return Err(io::Error::other(format!(
                        "rigid dormancy map failed: {error}"
                    )));
                }
            };
            let states: Vec<[f32; 3]> = bytes
                .chunks_exact(16)
                .map(|b| {
                    [
                        f32::from_bits(u32::from_le_bytes(b[0..4].try_into().unwrap())),
                        f32::from_bits(u32::from_le_bytes(b[4..8].try_into().unwrap())),
                        f32::from_bits(u32::from_le_bytes(b[8..12].try_into().unwrap())),
                    ]
                })
                .collect();
            debug_assert_eq!(
                states.len(),
                batch
                    .bodies
                    .iter()
                    .map(|body| body.cells.len())
                    .sum::<usize>()
            );
            drop(bytes);
            self.rigid_dormancy_readbacks[batch.readback_slot].unmap();
            self.rigid_dormancy_readback_free.push(batch.readback_slot);
            let mut cursor = 0;
            let mut bodies = batch.bodies.into_iter();
            let mut records = Vec::new();
            while let Some(body) = bodies.next() {
                let end = cursor + body.cells.len();
                let record = crate::scenes::DormantRigidBody {
                    id: body.id,
                    position: body.position,
                    rotation: body.rotation,
                    linear_velocity: body.linear_velocity,
                    angular_velocity: body.angular_velocity,
                    sleeping: body.sleeping,
                    cells: body
                        .cells
                        .iter()
                        .zip(&states[cursor..end])
                        .map(|(cell, state)| crate::scenes::DormantRigidCell {
                            local: cell.local,
                            material: cell.material,
                            appearance: cell.appearance,
                            integrity: state[0],
                            amount: state[1],
                            temperature: state[2],
                        })
                        .collect(),
                };
                cursor = end;
                if let Err(error) = record.validate(self.data.materials()) {
                    let mut pending = records
                        .into_iter()
                        .map(|(body, _)| body)
                        .collect::<Vec<_>>();
                    pending.push(body);
                    pending.extend(bodies);
                    self.restore_aborted_rigid_dormancy(pending);
                    return Err(error);
                }
                records.push((body, record));
            }
            for (body, record) in records {
                self.rigid_persistence_queue.push_back(RigidIoJob::Persist(
                    RigidPersistenceRequest {
                        slots: body.cells.iter().map(|cell| cell.state_slot).collect(),
                        record,
                    },
                ));
            }
            debug_assert_eq!(cursor, states.len());
            self.rigid_io_submit();
        }
        Ok(())
    }

    /// A failed map leaves the live Accelerator slots untouched, so reinserting the
    /// frozen CPU snapshot restores the only authoritative resident body.
    fn restore_aborted_rigid_dormancy(&mut self, pending: Vec<PendingRigidDormancy>) {
        for pending in pending {
            let (friction, restitution) = self.rigid_cellular_material_response(&pending.cells);
            let mut body = self.physics_world.insert_rigid_cellular_body(
                pending.position,
                pending.rotation,
                self.data.materials(),
                pending.cells,
                friction,
                restitution,
                pending.linear_velocity,
                pending.angular_velocity,
            );
            body.id = pending.id;
            if pending.sleeping {
                self.physics_world.sleep_rigid_cellular_body(&body);
            }
            self.rigid_cellular_bodies.push(body);
        }
        self.rigid_cellular_topology_revision =
            self.rigid_cellular_topology_revision.wrapping_add(1);
        self.rigid_cellular_contact_active
            .resize(self.rigid_cellular_bodies.len(), false);
        self.rigid_granular_contact_active
            .resize(self.rigid_cellular_bodies.len(), false);
        self.debug_assert_rigid_resident_invariants();
    }

    fn rigid_owner_load(&mut self, owner: TileCoordinates) {
        if self.rigid_owner_loads.contains_key(&owner)
            || self.rigid_owner_load_queue.contains(&owner)
        {
            return;
        }
        self.rigid_owner_load_queue.push_back(owner);
        self.rigid_io_submit();
    }

    /// Starts at most two owner-file operations.  This is deliberately a
    /// bounded worker frontier: disk latency delays an area shift, never a frame.
    fn rigid_io_submit(&mut self) {
        while self.rigid_io_in_flight < RIGID_IO_MAX_IN_FLIGHT {
            let job = if let Some(owner) = self.rigid_owner_load_queue.pop_front() {
                self.rigid_owner_loads
                    .insert(owner, RigidOwnerLoad::Loading);
                let generation = *self.rigid_owner_generation.entry(owner).or_default();
                let data = self.data.clone();
                let sender = self.rigid_streaming_response_sender.clone();
                self.rigid_io_in_flight += 1;
                std::thread::spawn(move || {
                    let _ = sender.send(RigidStreamingResponse::Loaded {
                        owner,
                        generation,
                        result: data.read_dormant_rigids(owner),
                    });
                });
                continue;
            } else if let Some(job) = self.rigid_persistence_queue.pop_front() {
                job
            } else {
                break;
            };
            let data = self.data.clone();
            let sender = self.rigid_streaming_response_sender.clone();
            self.rigid_io_in_flight += 1;
            std::thread::spawn(move || match job {
                RigidIoJob::Persist(request) => {
                    let Some(owner) = crate::scenes::owner_chunk(
                        request.record.position,
                        request.record.rotation,
                        request.record.cells.iter().map(|cell| cell.local),
                    ) else {
                        let _ = sender.send(RigidStreamingResponse::Saved {
                            request,
                            result: Err(io::Error::new(
                                io::ErrorKind::InvalidData,
                                "invalid rigid geometry",
                            )),
                        });
                        return;
                    };
                    let result = data.read_dormant_rigids(owner).and_then(|mut records| {
                        crate::scenes::append_record(&mut records, request.record.clone())?;
                        data.write_dormant_rigids(owner, &records)
                    });
                    let _ = sender.send(RigidStreamingResponse::Saved { request, result });
                }
                RigidIoJob::Claim {
                    owner,
                    original,
                    restored_ids,
                } => {
                    let result = data.read_dormant_rigids(owner).and_then(|mut records| {
                        crate::scenes::remove_ids(&mut records, &restored_ids);
                        data.write_dormant_rigids(owner, &records)?;
                        Ok(records)
                    });
                    let _ = sender.send(RigidStreamingResponse::Claimed {
                        owner,
                        original,
                        restored_ids,
                        result,
                    });
                }
            });
        }
    }

    fn rigid_streaming_apply_completed(&mut self) -> Result<(), io::Error> {
        while let Ok(response) = self.rigid_streaming_responses.try_recv() {
            self.rigid_io_in_flight = self.rigid_io_in_flight.saturating_sub(1);
            match response {
                RigidStreamingResponse::Loaded {
                    owner,
                    generation,
                    result,
                } if self
                    .rigid_owner_generation
                    .get(&owner)
                    .copied()
                    .unwrap_or_default()
                    == generation =>
                {
                    match result {
                        Ok(records) => {
                            self.rigid_owner_loads
                                .insert(owner, RigidOwnerLoad::Ready(records));
                        }
                        Err(error) => {
                            self.rigid_owner_loads.remove(&owner);
                            tracing::warn!(
                                owner_x = owner.x,
                                owner_y = owner.y,
                                "dormant rigid load failed: {error}"
                            );
                        }
                    }
                }
                RigidStreamingResponse::Loaded { owner, .. } => {
                    // A newer generation may already be loading. If not,
                    // stale completion must repair the desired-owner state.
                    if self.rigid_desired_owners.contains(&owner)
                        && !matches!(
                            self.rigid_owner_loads.get(&owner),
                            Some(RigidOwnerLoad::Loading)
                                | Some(RigidOwnerLoad::Ready(_))
                                | Some(RigidOwnerLoad::Claiming)
                        )
                    {
                        self.rigid_owner_load(owner);
                    }
                }
                RigidStreamingResponse::Saved { request, result } => match result {
                    Ok(()) => {
                        let owner = crate::scenes::owner_chunk(
                            request.record.position,
                            request.record.rotation,
                            request.record.cells.iter().map(|cell| cell.local),
                        )
                        .ok_or_else(|| {
                            io::Error::new(io::ErrorKind::InvalidData, "invalid rigid geometry")
                        })?;
                        let generation = self.rigid_owner_generation.entry(owner).or_default();
                        *generation = generation.wrapping_add(1);
                        let claiming = matches!(
                            self.rigid_owner_loads.get(&owner),
                            Some(RigidOwnerLoad::Claiming)
                        );
                        if !claiming {
                            self.rigid_owner_loads.remove(&owner);
                        }
                        for slot in request.slots {
                            self.release_rigid_cell_state(slot);
                        }
                        if !claiming && self.rigid_desired_owners.contains(&owner) {
                            self.rigid_owner_load(owner);
                        }
                    }
                    Err(error) => {
                        self.restore_failed_rigid_persistence(request)?;
                        return Err(error);
                    }
                },
                RigidStreamingResponse::Claimed {
                    owner,
                    original,
                    restored_ids,
                    result,
                } => match result {
                    Ok(records) => {
                        self.rigid_owner_loads
                            .insert(owner, RigidOwnerLoad::Ready(records));
                    }
                    Err(error) => {
                        self.rollback_rigid_restore(&restored_ids);
                        self.rigid_owner_loads
                            .insert(owner, RigidOwnerLoad::Ready(original));
                        if self.rigid_desired_owners.contains(&owner) {
                            self.rigid_owner_loads.remove(&owner);
                            self.rigid_owner_generation
                                .entry(owner)
                                .and_modify(|generation| *generation = generation.wrapping_add(1));
                            self.rigid_owner_load(owner);
                        }
                        return Err(error);
                    }
                },
            }
        }
        self.rigid_io_submit();
        Ok(())
    }

    fn restore_failed_rigid_persistence(
        &mut self,
        request: RigidPersistenceRequest,
    ) -> Result<(), io::Error> {
        let mut cells = Vec::with_capacity(request.record.cells.len());
        for (cell, slot) in request.record.cells.iter().zip(request.slots) {
            cells.push(RigidCellularBodyCell {
                local: cell.local,
                material: cell.material,
                appearance: cell.appearance,
                state_slot: slot,
                state_generation: self.rigid_cell_state_generations[slot as usize],
            });
        }
        let (friction, restitution) = self.rigid_cellular_material_response(&cells);
        let mut body = self.physics_world.insert_rigid_cellular_body(
            request.record.position,
            request.record.rotation,
            self.data.materials(),
            cells,
            friction,
            restitution,
            request.record.linear_velocity,
            request.record.angular_velocity,
        );
        body.id = request.record.id;
        if request.record.sleeping {
            self.physics_world.sleep_rigid_cellular_body(&body);
        }
        self.rigid_cellular_bodies.push(body);
        self.rigid_cellular_topology_revision =
            self.rigid_cellular_topology_revision.wrapping_add(1);
        self.rigid_cellular_contact_active
            .resize(self.rigid_cellular_bodies.len(), false);
        self.rigid_granular_contact_active
            .resize(self.rigid_cellular_bodies.len(), false);
        self.debug_assert_rigid_resident_invariants();
        Ok(())
    }

    fn rollback_rigid_restore(&mut self, ids: &[u64]) {
        for id in ids {
            if let Some(index) = self
                .rigid_cellular_bodies
                .iter()
                .position(|body| body.id == *id)
            {
                let body = self.rigid_cellular_bodies.swap_remove(index);
                self.rigid_activation_pending.remove(&body.id);
                self.rigid_sleeping_pending.remove(&body.id);
                self.physics_world.remove_rigid_cellular_body(&body);
                for cell in body.cells {
                    self.release_rigid_cell_state(cell.state_slot);
                }
            }
        }
        self.rigid_cellular_topology_revision =
            self.rigid_cellular_topology_revision.wrapping_add(1);
        self.rigid_cellular_contact_active
            .resize(self.rigid_cellular_bodies.len(), false);
        self.rigid_granular_contact_active
            .resize(self.rigid_cellular_bodies.len(), false);
    }

    /// Runs after incoming cellular/fluid/gas uploads have been submitted and
    /// before `update` can enter a fixed simulation tick.
    fn restore_ready_rigids(&mut self) -> Result<(), io::Error> {
        let owners: Vec<_> = self
            .rigid_owner_loads
            .iter()
            .filter_map(|(owner, state)| {
                matches!(state, RigidOwnerLoad::Ready(_)).then_some(*owner)
            })
            .collect();
        for owner in owners {
            self.restore_dormant_rigids(owner)?;
        }
        Ok(())
    }

    /// Claims loaded records only after terrain uploads were submitted.  The
    /// owner file remains authoritative until the background claim completes.
    fn restore_dormant_rigids(&mut self, owner: TileCoordinates) -> Result<(), io::Error> {
        let Some(RigidOwnerLoad::Ready(original)) = self.rigid_owner_loads.remove(&owner) else {
            return Ok(());
        };
        let buffered = self.area_buffered();
        let (records, _retained): (Vec<_>, Vec<_>) = original.iter().cloned().partition(|record| {
            crate::scenes::world_aabb(
                record.position,
                record.rotation,
                record.cells.iter().map(|cell| cell.local),
            )
            .is_some_and(|bounds| crate::scenes::intersects_area(bounds, buffered))
        });
        if records.is_empty() {
            self.rigid_owner_loads
                .insert(owner, RigidOwnerLoad::Ready(original));
            return Ok(());
        }
        let required: usize = records.iter().map(|record| record.cells.len()).sum();
        if required > self.rigid_cell_state_free.len() {
            self.rigid_owner_loads
                .insert(owner, RigidOwnerLoad::Ready(original));
            return Ok(());
        }
        for record in &records {
            record.validate(self.data.materials())?;
            if self
                .rigid_cellular_bodies
                .iter()
                .any(|body| body.id == record.id)
            {
                self.rigid_owner_loads
                    .insert(owner, RigidOwnerLoad::Ready(original));
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "duplicate resident rigid identity",
                ));
            }
        }
        let mut uploaded = Vec::with_capacity(required);
        let mut ids = Vec::with_capacity(records.len());
        for record in &records {
            let mut cells = Vec::with_capacity(record.cells.len());
            for cell in &record.cells {
                let slot = self.rigid_cell_state_free.pop().expect("capacity checked");
                let generation = self.rigid_cell_state_generations[slot as usize];
                cells.push(RigidCellularBodyCell {
                    local: cell.local,
                    material: cell.material,
                    appearance: cell.appearance,
                    state_slot: slot,
                    state_generation: generation,
                });
                uploaded.push([
                    slot,
                    cell.integrity.to_bits(),
                    cell.amount.to_bits(),
                    cell.temperature.to_bits(),
                ]);
            }
            let (friction, restitution) = self.rigid_cellular_material_response(&cells);
            let mut body = self.physics_world.insert_rigid_cellular_body(
                record.position,
                record.rotation,
                self.data.materials(),
                cells,
                friction,
                restitution,
                record.linear_velocity,
                record.angular_velocity,
            );
            body.id = record.id;
            if record.sleeping {
                self.rigid_sleeping_pending.insert(body.id);
            }
            self.physics_world
                .set_rigid_cellular_body_enabled(&body, false);
            self.rigid_activation_pending.insert(body.id);
            ids.push(body.id);
            self.rigid_cellular_body_id_next =
                self.rigid_cellular_body_id_next
                    .max(record.id.checked_add(1).ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidData, "rigid identity overflow")
                    })?);
            self.rigid_cellular_bodies.push(body);
        }
        self.rigid_cell_state_upload
            .apply(self.accelerator.as_ref(), &uploaded);
        self.rigid_cellular_topology_revision =
            self.rigid_cellular_topology_revision.wrapping_add(1);
        self.rigid_cellular_contact_active
            .resize(self.rigid_cellular_bodies.len(), false);
        self.rigid_granular_contact_active
            .resize(self.rigid_cellular_bodies.len(), false);
        self.rigid_owner_loads
            .insert(owner, RigidOwnerLoad::Claiming);
        self.rigid_persistence_queue.push_back(RigidIoJob::Claim {
            owner,
            original,
            restored_ids: ids,
        });
        self.rigid_io_submit();
        self.debug_assert_rigid_resident_invariants();
        Ok(())
    }

    /// Runs one fixed-rate physics simulation tick
    fn tick(&mut self, is_simulation_active: bool) -> Result<(), io::Error> {
        if let Some(mut snapshot) = self.cellular_collision.latest.take() {
            let age = self.cellular_collision.snapshot_age(snapshot.sequence);
            self.physics_world.set_collision_snapshot_age(age);
            self.detach_unanchored_static_components(&mut snapshot)?;
            let collision_matches_current_ring = snapshot.origin == self.area_buffered().origin();
            let snapshot_origin = snapshot.origin;
            self.physics_world.update_cellular_snapshot(snapshot);
            if collision_matches_current_ring {
                self.rigid_activation_collision_origin = Some(snapshot_origin);
            }
        }
        let delta_time: f32 = 1.0 / TICK_RATE as f32;
        let actor_proxies = self.actor_registry.cellular_proxy_states();
        if is_simulation_active {
            let up = if self.gravity[0].hypot(self.gravity[1]) > 0.0 {
                Vector::new(-self.gravity[0], -self.gravity[1]).normalize()
            } else {
                Vector::Y
            };
            self.physics_world
                .sync_pawn_proxies(&self.actor_registry.physics_proxy_states(), up);
            self.physics_world.prepare_cellular_terrain(
                &self.rigid_cellular_bodies,
                &actor_proxies,
                self.gravity,
                delta_time,
            );
            // `prepare_cellular_terrain` has now rebuilt the actual Rapier
            // terrain colliders from the matching snapshot.  Only then can a
            // staged body take part in this step.
            if self.rigid_activation_collision_origin == Some(self.area_buffered().origin()) {
                for body in &self.rigid_cellular_bodies {
                    if self.rigid_activation_pending.remove(&body.id) {
                        self.physics_world
                            .set_rigid_cellular_body_enabled(body, true);
                        if self.rigid_sleeping_pending.remove(&body.id) {
                            self.physics_world.sleep_rigid_cellular_body(body);
                        }
                    }
                }
            }
            self.physics_world.step(self.gravity, delta_time);
        }
        self.actor_registry.simulate_actor_pawns(
            1.0 / TICK_RATE as f32,
            is_simulation_active,
            self.gravity,
            &self.physics_world,
        );
        let up = if self.gravity[0].hypot(self.gravity[1]) > 0.0 {
            Vector::new(-self.gravity[0], -self.gravity[1]).normalize()
        } else {
            Vector::Y
        };
        self.physics_world
            .sync_pawn_proxies(&self.actor_registry.physics_proxy_states(), up);
        let actor_proxies = self.actor_registry.cellular_proxy_states();
        let possessed_position: Option<ScenePosition> = self
            .possessed_actor()
            .and_then(|actor| self.actor_registry.get_position(actor))
            .copied();
        if let Some(position) = possessed_position {
            self.follow_position(position);
        }
        let buffer_size: i32 = i32::from(self.simulation_buffer_size);
        let dimensions: u16 = u16::from(self.simulation_buffer_size) * 2;
        let rigid_body_states: Vec<RigidCellularBodyState> = self
            .rigid_cellular_bodies
            .iter()
            .map(|body| {
                self.physics_world
                    .rigid_cellular_body_state(body)
                    .ok_or_else(|| io::Error::other("Rigid cellular body handle is missing"))
            })
            .collect::<Result<_, _>>()?;
        self.cellular_physics_body_proxy.rasterize(
            self.accelerator.as_ref(),
            TileCoordinates {
                x: self.origin.x - buffer_size,
                y: self.origin.y - buffer_size,
            },
            self.simulation_width + dimensions,
            self.simulation_height + dimensions,
            self.tiles_ring_offset_x,
            self.tiles_ring_offset_y,
            self.gravity,
            &actor_proxies,
            &self.rigid_cellular_bodies,
            &rigid_body_states,
            self.rigid_cellular_topology_revision,
        );
        if is_simulation_active {
            let fluid_active_area: TileArea = self.area_fluid_active();
            let fluid_active_dimensions: [u16; 2] = fluid_active_area.dimensions();
            self.fluids.simulate(
                self.accelerator.as_ref(),
                fluid_active_area.origin(),
                fluid_active_dimensions[0],
                fluid_active_dimensions[1],
                TileCoordinates {
                    x: self.origin.x - buffer_size,
                    y: self.origin.y - buffer_size,
                },
                self.simulation_width + dimensions,
                self.simulation_height + dimensions,
                self.tiles_ring_offset_x,
                self.tiles_ring_offset_y,
                self.gravity,
                1.0 / TICK_RATE as f32,
            );
            self.gases.simulate_pre_coupling(
                self.accelerator.as_ref(),
                TileCoordinates {
                    x: self.origin.x - buffer_size,
                    y: self.origin.y - buffer_size,
                },
                self.simulation_width + dimensions,
                self.simulation_height + dimensions,
                self.tiles_ring_offset_x,
                self.tiles_ring_offset_y,
                self.gravity,
                1.0 / TICK_RATE as f32,
            );
            // Chemistry discovery observes the post-advection material snapshot
            // and the prior resolved pressure field. Its outputs are applied in
            // later stages, never recursively during this discovery pass.
            {
                let mut encoder = self.accelerator.wgpu_device().create_command_encoder(
                    &wgpu::CommandEncoderDescriptor {
                        label: Some("material reaction discovery"),
                    },
                );
                self.material_reactions
                    .encode(self.accelerator.as_ref(), &mut encoder);
                // Resolve chemistry's authority-addressed cell mutations before
                // pressure observes this tick's post-reaction material snapshot.
                self.material_mutations.encode_requests(
                    self.accelerator.as_ref(),
                    &mut encoder,
                    u32::from(self.simulation_width + dimensions)
                        * u32::from(self.simulation_height + dimensions)
                        * 64,
                    self.gases.gas_count(),
                );
                self.accelerator.wgpu_queue().submit(Some(encoder.finish()));
                self.material_reactions
                    .submit_rigid_removal_readback(self.accelerator.as_ref());
            }
            self.cellular_pressure.simulate(
                self.accelerator.as_ref(),
                TileCoordinates {
                    x: self.origin.x - buffer_size,
                    y: self.origin.y - buffer_size,
                },
                self.simulation_width + dimensions,
                self.simulation_height + dimensions,
                self.tiles_ring_offset_x,
                self.tiles_ring_offset_y,
                1.0 / TICK_RATE as f32,
                self.gravity,
                self.rigid_cellular_bodies.len(),
                self.cellular_physics_body_proxy
                    .rigid_cell_count(&self.rigid_cellular_bodies),
                self.rigid_cellular_topology_revision,
            )?;
            self.material_mutations.resolve_requests(
                self.accelerator.as_ref(),
                u32::from(self.simulation_width + dimensions)
                    * u32::from(self.simulation_height + dimensions)
                    * 64,
                self.gases.gas_count(),
            );
            self.fluids.consume_accelerator_edits(
                self.accelerator.as_ref(),
                fluid_active_area.origin(),
                fluid_active_dimensions[0],
                fluid_active_dimensions[1],
                TileCoordinates {
                    x: self.origin.x - buffer_size,
                    y: self.origin.y - buffer_size,
                },
                self.simulation_width + dimensions,
                self.simulation_height + dimensions,
                self.tiles_ring_offset_x,
                self.tiles_ring_offset_y,
            );
            self.cellular_dynamic.simulate_cellular_dynamic_tick(
                self.accelerator.as_ref(),
                &self.cellular_material_identifiers,
                &self.cellular_appearances,
                &self.cellular_amounts,
                &self.cellular_temperatures,
                TileCoordinates {
                    x: self.origin.x - buffer_size,
                    y: self.origin.y - buffer_size,
                },
                self.simulation_width + dimensions,
                self.simulation_height + dimensions,
                self.tiles_ring_offset_x,
                self.tiles_ring_offset_y,
                self.gravity,
                1.0 / TICK_RATE as f32,
            );
            self.fluids
                .scatter_mechanical_response(self.accelerator.as_ref());
            self.gases.simulate_post_coupling(self.accelerator.as_ref());
            let thermal_origin = [
                self.origin.x - i32::from(self.simulation_buffer_size),
                self.origin.y - i32::from(self.simulation_buffer_size),
            ];
            let thermal_tiles = [
                u32::from(self.simulation_width + u16::from(self.simulation_buffer_size) * 2),
                u32::from(self.simulation_height + u16::from(self.simulation_buffer_size) * 2),
            ];
            let thermal_ring = [
                u32::from(self.tiles_ring_offset_x),
                u32::from(self.tiles_ring_offset_y),
            ];
            let mut thermal_encoder = self.accelerator.wgpu_device().create_command_encoder(
                &wgpu::CommandEncoderDescriptor {
                    label: Some("thermal pipeline"),
                },
            );
            self.thermal_interaction.encode(
                self.accelerator.as_ref(),
                &mut thermal_encoder,
                thermal_origin,
                thermal_tiles,
                thermal_ring,
                !self.rigid_cellular_bodies.is_empty(),
            );
            self.thermal_conduction.encode(
                self.accelerator.as_ref(),
                &mut thermal_encoder,
                thermal_origin,
                thermal_tiles,
                thermal_ring,
                1.0 / TICK_RATE as f32,
            );
            self.thermal_scatter.encode(
                self.accelerator.as_ref(),
                &mut thermal_encoder,
                thermal_origin,
                thermal_tiles,
                thermal_ring,
                !self.rigid_cellular_bodies.is_empty(),
            );
            self.thermal_phase_transitions.encode(
                self.accelerator.as_ref(),
                &mut thermal_encoder,
                thermal_origin,
                [
                    u32::from(self.simulation_width + dimensions),
                    u32::from(self.simulation_height + dimensions),
                ],
                thermal_ring,
                self.cellular_physics_body_proxy
                    .rigid_cell_count(&self.rigid_cellular_bodies) as u32,
            );
            self.material_mutations.encode_requests(
                self.accelerator.as_ref(),
                &mut thermal_encoder,
                u32::from(self.simulation_width + dimensions)
                    * u32::from(self.simulation_height + dimensions)
                    * 64,
                self.gases.gas_count(),
            );
            self.material_mutations.encode_thermal_condensation(
                self.accelerator.as_ref(),
                &mut thermal_encoder,
                u32::from(self.simulation_width + dimensions)
                    * u32::from(self.simulation_height + dimensions)
                    * 64,
                self.gases.gas_count(),
            );
            self.accelerator
                .wgpu_queue()
                .submit(Some(thermal_encoder.finish()));
            self.thermal_phase_transitions.submit_rigid_readback(
                self.accelerator.as_ref(),
                self.cellular_physics_body_proxy
                    .rigid_cell_count(&self.rigid_cellular_bodies) as u32,
            );
            self.fluid_sample_submit()?;
            self.cellular_collision_dirty = true;
        }
        if !self.cellular_collision_dirty {
            return Ok(());
        }
        let buffer_size: i32 = i32::from(self.simulation_buffer_size);
        if self.cellular_collision.extract_collision(
            self.accelerator.as_ref(),
            TileCoordinates {
                x: self.origin.x - buffer_size,
                y: self.origin.y - buffer_size,
            },
            self.simulation_width + u16::from(self.simulation_buffer_size) * 2,
            self.simulation_height + u16::from(self.simulation_buffer_size) * 2,
            self.tiles_ring_offset_x,
            self.tiles_ring_offset_y,
        )? {
            self.cellular_collision_dirty = false;
        }
        Ok(())
    }

    /// Transfers newly disconnected static components into authoritative body-local matter
    fn apply_completed_static_detachment(&mut self) -> Result<(), io::Error> {
        let Some(result) = self.cellular_static_state_gather.take_completed() else {
            return Ok(());
        };
        let Some(in_flight_generation) = self.static_detachment_in_flight_generation.take() else {
            return Ok(());
        };
        let Some(pending) = self.pending_static_detachment.take() else {
            return Ok(());
        };
        if pending.generation != in_flight_generation {
            #[cfg(debug_assertions)]
            tracing::trace!(
                target: "engine_physics::static_detachment",
                generation = in_flight_generation,
                "discarded stale static detachment gather"
            );
            self.pending_static_detachment = Some(pending);
            return Ok(());
        }
        let states = match result {
            Ok(states) if states.len() == pending.indices.len() => states,
            _ => {
                self.pending_static_detachment = Some(pending);
                return Ok(());
            }
        };
        let current_indices: Option<Vec<u32>> = pending
            .components
            .iter()
            .flatten()
            .map(|coordinates| self.cell_edit_index(*coordinates).map(|index| index as u32))
            .collect();
        if current_indices.as_deref() != Some(&pending.indices) {
            self.static_detachment_generation = self.static_detachment_generation.wrapping_add(1);
            self.pending_static_detachment =
                current_indices.map(|indices| PendingStaticDetachment {
                    components: pending.components,
                    indices,
                    generation: self.static_detachment_generation,
                    ring_offset: (self.tiles_ring_offset_x, self.tiles_ring_offset_y),
                });
            return Ok(());
        }
        if pending.ring_offset != (self.tiles_ring_offset_x, self.tiles_ring_offset_y) {
            self.static_detachment_generation = self.static_detachment_generation.wrapping_add(1);
            self.pending_static_detachment = Some(PendingStaticDetachment {
                components: pending.components,
                indices: current_indices.unwrap_or_default(),
                generation: self.static_detachment_generation,
                ring_offset: (self.tiles_ring_offset_x, self.tiles_ring_offset_y),
            });
            return Ok(());
        }
        let mut offset = 0;
        let mut edits = SceneEditBatch::new();
        let mut rigid_insertions = Vec::new();
        for component in pending.components {
            let end = offset + component.len();
            let component_states = &states[offset..end];
            offset = end;
            if component_states.iter().any(|state| {
                state.amount <= 0.000001
                    || !matches!(
                        self.data
                            .materials()
                            .get(MaterialIdentifier::from_u32(state.material)),
                        Some(Material::CellularStatic { .. })
                    )
            }) {
                continue;
            }
            let minimum_x = component.iter().map(|cell| cell.x).min().unwrap();
            let minimum_y = component.iter().map(|cell| cell.y).min().unwrap();
            let mut cells = Vec::with_capacity(component.len());
            let mut integrities = Vec::with_capacity(component.len());
            let mut amounts = Vec::with_capacity(component.len());
            let mut temperatures = Vec::with_capacity(component.len());
            let mut friction = 0.0;
            let mut restitution = 0.0;
            for (coordinates, state) in component.iter().zip(component_states) {
                let material_identifier = MaterialIdentifier::from_u32(state.material);
                let Some(Material::CellularStatic {
                    friction: cell_friction,
                    restitution: cell_restitution,
                    ..
                }) = self.data.materials().get(material_identifier)
                else {
                    cells.clear();
                    break;
                };
                friction += *cell_friction;
                restitution += *cell_restitution;
                cells.push(RigidCellularBodyCell {
                    local: [coordinates.x - minimum_x, coordinates.y - minimum_y],
                    material: material_identifier,
                    appearance: CellularAppearance(state.appearance),
                    state_slot: u32::MAX,
                    state_generation: 0,
                });
                integrities.push(state.integrity);
                amounts.push(state.amount);
                temperatures.push(state.temperature);
            }
            if cells.len() != component.len() {
                continue;
            }
            if cells.len() < self.rigid_component_minimum(&cells) {
                let debris = cells
                    .iter()
                    .filter_map(|cell| match self.data.materials().get(cell.material) {
                        Some(Material::CellularStatic {
                            debris_material: Some(material_identifier),
                            debris_yield_rate,
                            ..
                        }) if (((cell.local[0].wrapping_mul(31).wrapping_add(cell.local[1])) as u32
                            % 10_000) as f32)
                            < *debris_yield_rate * 10_000.0 =>
                        {
                            Some(SceneEditCellPlacement {
                                coordinates: CellCoordinates {
                                    x: minimum_x + cell.local[0],
                                    y: minimum_y + cell.local[1],
                                },
                                material_identifier: *material_identifier,
                                appearance: cell.appearance,
                            })
                        }
                        _ => None,
                    })
                    .collect();
                edits.erase(component.clone());
                edits.place_cells(debris);
                continue;
            }
            let divisor = cells.len() as f32;
            edits.erase(component.clone());
            rigid_insertions.push((
                [minimum_x as f32 / 8.0, minimum_y as f32 / 8.0],
                cells,
                friction / divisor,
                restitution / divisor,
                integrities
                    .iter()
                    .copied()
                    .zip(amounts.iter().copied().zip(temperatures.iter().copied()))
                    .map(|(integrity, (amount, temperature))| (integrity, amount, temperature))
                    .collect::<Vec<_>>(),
            ));
        }
        if !edits.is_empty() {
            self.apply_edits_immediate(&mut edits)?;
        }
        for (position, cells, friction, restitution, state) in rigid_insertions {
            self.insert_rigid_cellular_body(position, cells, friction, restitution, Some(&state));
        }
        #[cfg(debug_assertions)]
        tracing::trace!(
            target: "engine_physics::static_detachment",
            resolved_cells = offset,
            edit_applied = !edits.is_empty(),
            "resolved static detachment batch"
        );
        Ok(())
    }

    fn submit_pending_static_detachment(&mut self) {
        if self.static_detachment_in_flight_generation.is_some() {
            return;
        }
        let Some(pending) = self.pending_static_detachment.as_ref() else {
            return;
        };
        if self
            .cellular_static_state_gather
            .submit(self.accelerator.as_ref(), &pending.indices)
        {
            self.static_detachment_in_flight_generation = Some(pending.generation);
            #[cfg(debug_assertions)]
            tracing::trace!(
                target: "engine_physics::static_detachment",
                generation = pending.generation,
                gather_cells = pending.indices.len(),
                "submitted static detachment gather"
            );
        }
    }

    fn detach_unanchored_static_components(
        &mut self,
        snapshot: &mut CollisionOccupancySnapshot,
    ) -> Result<(), io::Error> {
        let Some(previous) = self.rigid_detachment_snapshot.take() else {
            self.rigid_detachment_snapshot = Some(snapshot.clone());
            return Ok(());
        };
        if previous.origin != snapshot.origin
            || previous.width != snapshot.width
            || previous.height != snapshot.height
        {
            self.rigid_detachment_snapshot = Some(snapshot.clone());
            if let Some(pending) = self.pending_static_detachment.take() {
                let indices = pending
                    .components
                    .iter()
                    .flatten()
                    .filter_map(|coordinates| self.cell_edit_index(*coordinates))
                    .map(|index| index as u32)
                    .collect::<Vec<_>>();
                self.static_detachment_generation =
                    self.static_detachment_generation.wrapping_add(1);
                self.pending_static_detachment =
                    (!indices.is_empty()).then_some(PendingStaticDetachment {
                        components: pending.components,
                        indices,
                        generation: self.static_detachment_generation,
                        ring_offset: (self.tiles_ring_offset_x, self.tiles_ring_offset_y),
                    });
            }
            self.submit_pending_static_detachment();
            return Ok(());
        }
        if previous.static_masks == snapshot.static_masks {
            self.submit_pending_static_detachment();
            self.rigid_detachment_snapshot = Some(snapshot.clone());
            return Ok(());
        }
        let origin_x: i32 = snapshot.origin.x * 8;
        let origin_y: i32 = snapshot.origin.y * 8;
        let width: i32 = i32::from(snapshot.width) * 8;
        let height: i32 = i32::from(snapshot.height) * 8;
        let mut seeds = Vec::new();
        let mut changed_bits = 0usize;
        for tile_y in 0..snapshot.height {
            for tile_x in 0..snapshot.width {
                let tile = usize::from(tile_y) * usize::from(snapshot.width) + usize::from(tile_x);
                for word in 0..2 {
                    let current = snapshot.static_masks[tile][word];
                    let previous = previous.static_masks[tile][word];
                    let added = current & !previous;
                    let removed = previous & !current;
                    changed_bits += (added | removed).count_ones() as usize;
                    for mask in [added, removed] {
                        let mut bits = mask;
                        while bits != 0 {
                            let bit = bits.trailing_zeros();
                            let local = word as i32 * 32 + bit as i32;
                            let cell = CellCoordinates {
                                x: (snapshot.origin.x + i32::from(tile_x)) * 8 + local % 8,
                                y: (snapshot.origin.y + i32::from(tile_y)) * 8 + local / 8,
                            };
                            if mask == added {
                                seeds.push(cell);
                            } else {
                                for neighbor in [
                                    CellCoordinates {
                                        x: cell.x - 1,
                                        y: cell.y,
                                    },
                                    CellCoordinates {
                                        x: cell.x + 1,
                                        y: cell.y,
                                    },
                                    CellCoordinates {
                                        x: cell.x,
                                        y: cell.y - 1,
                                    },
                                    CellCoordinates {
                                        x: cell.x,
                                        y: cell.y + 1,
                                    },
                                ] {
                                    if snapshot.is_static_cell_occupied(neighbor.x, neighbor.y)
                                        == Some(true)
                                    {
                                        seeds.push(neighbor);
                                    }
                                }
                            }
                            bits &= bits - 1;
                        }
                    }
                }
            }
        }
        seeds.sort_unstable_by_key(|cell| (cell.y, cell.x));
        seeds.dedup();
        let visit_count = (width * height) as usize;
        if self.static_detachment_visit_stamps.len() != visit_count {
            self.static_detachment_visit_stamps = vec![0; visit_count];
            self.static_detachment_visit_generation = 0;
        }
        self.static_detachment_visit_generation = self
            .static_detachment_visit_generation
            .wrapping_add(1)
            .max(1);
        let visit_generation = self.static_detachment_visit_generation;
        let visit_index =
            |cell: CellCoordinates| ((cell.y - origin_y) * width + cell.x - origin_x) as usize;
        let mut candidates = Vec::new();
        let mut visited_cells = 0usize;
        for seed in seeds.iter().copied() {
            if snapshot.is_static_cell_occupied(seed.x, seed.y) != Some(true)
                || self.static_detachment_visit_stamps[visit_index(seed)] == visit_generation
            {
                continue;
            }
            let mut queue = vec![seed];
            self.static_detachment_visit_stamps[visit_index(seed)] = visit_generation;
            let mut component = Vec::new();
            let mut cursor = 0;
            let mut anchored = false;
            while cursor < queue.len() {
                let cell = queue[cursor];
                cursor += 1;
                component.push(cell);
                visited_cells += 1;
                anchored |= cell.x == origin_x
                    || cell.y == origin_y
                    || cell.x == origin_x + width - 1
                    || cell.y == origin_y + height - 1;
                if anchored || component.len() > RIGID_DETACHMENT_MAXIMUM_CELLS {
                    break;
                }
                for neighbor in [
                    CellCoordinates {
                        x: cell.x - 1,
                        y: cell.y,
                    },
                    CellCoordinates {
                        x: cell.x + 1,
                        y: cell.y,
                    },
                    CellCoordinates {
                        x: cell.x,
                        y: cell.y - 1,
                    },
                    CellCoordinates {
                        x: cell.x,
                        y: cell.y + 1,
                    },
                ] {
                    if neighbor.x < origin_x
                        || neighbor.y < origin_y
                        || neighbor.x >= origin_x + width
                        || neighbor.y >= origin_y + height
                        || snapshot.is_static_cell_occupied(neighbor.x, neighbor.y) != Some(true)
                    {
                        continue;
                    }
                    let index = visit_index(neighbor);
                    if self.static_detachment_visit_stamps[index] != visit_generation {
                        self.static_detachment_visit_stamps[index] = visit_generation;
                        queue.push(neighbor);
                    }
                }
            }
            if !anchored && component.len() <= RIGID_DETACHMENT_MAXIMUM_CELLS {
                candidates.push(component);
            }
        }
        #[cfg(debug_assertions)]
        tracing::trace!(
            target: "engine_physics::static_detachment",
            changed_bits,
            seed_count = seeds.len(),
            visited_cells,
            candidate_components = candidates.len(),
            candidate_cells = candidates.iter().map(Vec::len).sum::<usize>(),
            "delta-seeded static detachment"
        );
        let newly_discovered_cells: HashSet<_> = candidates.iter().flatten().copied().collect();
        let mut desired_components = self
            .pending_static_detachment
            .take()
            .map(|pending| {
                pending
                    .components
                    .into_iter()
                    .filter(|component| {
                        component.iter().all(|cell| {
                            snapshot.is_static_cell_occupied(cell.x, cell.y) == Some(true)
                                && !newly_discovered_cells.contains(cell)
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        desired_components.extend(candidates);
        if desired_components.is_empty() {
            self.rigid_detachment_snapshot = Some(snapshot.clone());
            self.submit_pending_static_detachment();
            return Ok(());
        }
        let mut indices = Vec::new();
        let mut valid_components = Vec::new();
        for component in desired_components {
            let Some(component_indices) = component
                .iter()
                .map(|coordinates| self.cell_edit_index(*coordinates).map(|index| index as u32))
                .collect::<Option<Vec<_>>>()
            else {
                continue;
            };
            indices.extend(component_indices);
            valid_components.push(component);
        }
        self.static_detachment_generation = self.static_detachment_generation.wrapping_add(1);
        self.pending_static_detachment = Some(PendingStaticDetachment {
            components: valid_components,
            indices,
            generation: self.static_detachment_generation,
            ring_offset: (self.tiles_ring_offset_x, self.tiles_ring_offset_y),
        });
        self.submit_pending_static_detachment();
        self.rigid_detachment_snapshot = Some(snapshot.clone());
        Ok(())
    }

    fn rigid_cell_debris_placement(
        &self,
        state: &RigidCellularBodyState,
        cell: &RigidCellularBodyCell,
    ) -> Option<SceneEditCellPlacement> {
        let Some(Material::CellularStatic {
            debris_material: Some(material_identifier),
            debris_yield_rate,
            ..
        }) = self.data.materials().get(cell.material)
        else {
            return None;
        };
        let seed = cell
            .state_slot
            .wrapping_mul(747_796_405)
            .wrapping_add(2_891_336_453);
        if (seed % 10_000) as f32 >= debris_yield_rate * 10_000.0 {
            return None;
        }
        let local = [
            (cell.local[0] as f32 + 0.5) / 8.0,
            (cell.local[1] as f32 + 0.5) / 8.0,
        ];
        let world = [
            state.translation[0] + state.angle.cos() * local[0] - state.angle.sin() * local[1],
            state.translation[1] + state.angle.sin() * local[0] + state.angle.cos() * local[1],
        ];
        Some(SceneEditCellPlacement {
            coordinates: CellCoordinates {
                x: (world[0] * 8.0).floor() as i32,
                y: (world[1] * 8.0).floor() as i32,
            },
            material_identifier: *material_identifier,
            appearance: cell.appearance,
        })
    }

    /// Applies Accelerator-detected rigid phase candidates after their bounded async
    /// readback.  Every candidate is rechecked against authoritative body
    /// state before its already-reserved PBF slot is committed.
    fn apply_completed_rigid_thermal_transitions(&mut self) {
        let Some(candidates) = self.thermal_phase_transitions.take_rigid_candidates() else {
            return;
        };
        let mut removals: HashMap<usize, HashSet<[i32; 2]>> = HashMap::new();
        let mut rollback = Vec::new();
        for candidate in candidates {
            let slot = candidate[0];
            let generation = candidate[1];
            let expected = MaterialIdentifier::from_u32(candidate[2]);
            let replacement = candidate[3];
            if !matches!(
                self.data
                    .materials()
                    .get(MaterialIdentifier::from_u32(replacement)),
                Some(Material::Fluid { .. })
            ) {
                rollback.push(candidate[9]);
                continue;
            }
            let amount = f32::from_bits(candidate[4]);
            let temperature = f32::from_bits(candidate[5]);
            let reserved = candidate[9];
            let Some((body_index, cell)) =
                self.rigid_cellular_bodies
                    .iter()
                    .enumerate()
                    .find_map(|(index, body)| {
                        body.cells
                            .iter()
                            .find(|cell| {
                                cell.state_slot == slot
                                    && cell.state_generation == generation
                                    && cell.material == expected
                            })
                            .copied()
                            .map(|cell| (index, cell))
                    })
            else {
                rollback.push(reserved);
                continue;
            };
            if !(amount >= 0.999 && amount <= 1.001)
                || self
                    .rigid_cell_state_generations
                    .get(slot as usize)
                    .copied()
                    != Some(generation)
            {
                rollback.push(reserved);
                continue;
            }
            let Some(state) = self
                .physics_world
                .rigid_cellular_body_state(&self.rigid_cellular_bodies[body_index])
            else {
                rollback.push(reserved);
                continue;
            };
            let local = [
                (cell.local[0] as f32 + 0.5) / 8.0,
                (cell.local[1] as f32 + 0.5) / 8.0,
            ];
            let offset = [
                state.angle.cos() * local[0] - state.angle.sin() * local[1],
                state.angle.sin() * local[0] + state.angle.cos() * local[1],
            ];
            let position = [
                state.translation[0] + offset[0],
                state.translation[1] + offset[1],
            ];
            let velocity = [
                state.linear_velocity[0]
                    - state.angular_velocity * (position[1] - state.center_of_mass[1]),
                state.linear_velocity[1]
                    + state.angular_velocity * (position[0] - state.center_of_mass[0]),
            ];
            self.fluids.commit_reserved_particle(
                self.accelerator.as_ref(),
                reserved,
                replacement,
                position,
                velocity,
                amount,
                temperature,
            );
            removals.entry(body_index).or_default().insert(cell.local);
        }
        if !rollback.is_empty() {
            self.thermal_phase_transitions
                .rollback_rigid_reservations(self.accelerator.as_ref(), &rollback);
        }
        let mut removals: Vec<_> = removals.into_iter().collect();
        removals.sort_unstable_by_key(|(body_index, _)| std::cmp::Reverse(*body_index));
        for (body_index, removed) in removals {
            self.remove_rigid_cellular_body_cells(
                body_index,
                &removed,
                RigidCellRemovalCause::PhaseTransition,
            );
        }
    }

    /// Removes body-local cells and replaces the body with its remaining connected pieces
    fn remove_rigid_cellular_body_cells(
        &mut self,
        body_index: usize,
        removed: &HashSet<[i32; 2]>,
        cause: RigidCellRemovalCause,
    ) {
        if body_index >= self.rigid_cellular_bodies.len() || removed.is_empty() {
            return;
        }
        let Some(state) = self
            .physics_world
            .rigid_cellular_body_state(&self.rigid_cellular_bodies[body_index])
        else {
            return;
        };
        let body = self.rigid_cellular_bodies.swap_remove(body_index);
        self.rigid_activation_pending.remove(&body.id);
        self.rigid_sleeping_pending.remove(&body.id);
        self.rigid_cellular_topology_revision =
            self.rigid_cellular_topology_revision.wrapping_add(1);
        self.rigid_cellular_support.clear();
        self.rigid_cellular_recovery.clear();
        self.rigid_cellular_contact_active.clear();
        self.rigid_granular_contact_active.clear();
        self.physics_world.remove_rigid_cellular_body(&body);
        let mut debris = Vec::new();
        for cell in body
            .cells
            .iter()
            .filter(|cell| removed.contains(&cell.local))
        {
            if cause == RigidCellRemovalCause::Fracture {
                if let Some(placement) = self.rigid_cell_debris_placement(&state, cell) {
                    debris.push(placement);
                }
            }
            self.release_rigid_cell_state(cell.state_slot);
        }
        let remaining = body
            .cells
            .into_iter()
            .filter(|cell| !removed.contains(&cell.local))
            .collect();
        for cells in RigidCellularBody::connected_components(remaining) {
            if cells.is_empty() {
                continue;
            }
            if cells.len() < self.rigid_component_minimum(&cells) {
                for cell in cells {
                    if let Some(placement) = self.rigid_cell_debris_placement(&state, &cell) {
                        debris.push(placement);
                    }
                    self.release_rigid_cell_state(cell.state_slot);
                }
                continue;
            }
            let local_center =
                RigidCellularBody::mass_properties(&cells, self.data.materials()).local_com;
            let child_center = [
                state.translation[0] + state.angle.cos() * local_center.x
                    - state.angle.sin() * local_center.y,
                state.translation[1]
                    + state.angle.sin() * local_center.x
                    + state.angle.cos() * local_center.y,
            ];
            let offset = [
                child_center[0] - state.center_of_mass[0],
                child_center[1] - state.center_of_mass[1],
            ];
            let child_velocity = [
                state.linear_velocity[0] - state.angular_velocity * offset[1],
                state.linear_velocity[1] + state.angular_velocity * offset[0],
            ];
            let (rebased_position, cells) =
                Self::rebase_rigid_cells(cells, state.translation, state.angle);
            let (friction, restitution) = self.rigid_cellular_material_response(&cells);
            let mut child = self.physics_world.insert_rigid_cellular_body(
                rebased_position,
                state.angle,
                self.data.materials(),
                cells,
                friction,
                restitution,
                child_velocity,
                state.angular_velocity,
            );
            child.id = self.next_rigid_cellular_body_id();
            self.rigid_cellular_bodies.push(child);
        }
        self.rigid_cellular_contact_active
            .resize(self.rigid_cellular_bodies.len(), false);
        self.rigid_granular_contact_active
            .resize(self.rigid_cellular_bodies.len(), false);
        self.debug_assert_rigid_resident_invariants();
        if !debris.is_empty() {
            self.pending_runtime_edits.place_cells(debris);
        }
    }

    /// Rebase a split component while preserving every cell's world position.
    fn rebase_rigid_cells(
        mut cells: Vec<RigidCellularBodyCell>,
        position: [f32; 2],
        angle: f32,
    ) -> ([f32; 2], Vec<RigidCellularBodyCell>) {
        let offset = cells
            .iter()
            .map(|cell| cell.local)
            .fold([i32::MAX, i32::MAX], |[min_x, min_y], [x, y]| {
                [min_x.min(x), min_y.min(y)]
            });
        for cell in &mut cells {
            cell.local[0] -= offset[0];
            cell.local[1] -= offset[1];
        }
        let (sin, cos) = angle.sin_cos();
        (
            [
                position[0] + (cos * offset[0] as f32 - sin * offset[1] as f32) / 8.0,
                position[1] + (sin * offset[0] as f32 + cos * offset[1] as f32) / 8.0,
            ],
            cells,
        )
    }

    /// Averages the existing static material response for one concrete body
    fn rigid_cellular_material_response(&self, cells: &[RigidCellularBodyCell]) -> (f32, f32) {
        let (friction, restitution) = cells.iter().fold((0.0, 0.0), |sum, cell| {
            match self.data.materials().get(cell.material) {
                Some(Material::CellularStatic {
                    friction,
                    restitution,
                    ..
                }) => (sum.0 + friction, sum.1 + restitution),
                _ => sum,
            }
        });
        let divisor = cells.len().max(1) as f32;
        (friction / divisor, restitution / divisor)
    }

    /// The strictest material in a mixed component controls its minimum size.
    fn rigid_component_minimum(&self, cells: &[RigidCellularBodyCell]) -> usize {
        cells
            .iter()
            .filter_map(|cell| match self.data.materials().get(cell.material) {
                Some(Material::CellularStatic {
                    minimum_rigid_body_cell_count,
                    ..
                }) => Some(*minimum_rigid_body_cell_count as usize),
                _ => None,
            })
            .max()
            .unwrap_or(1)
    }

    fn release_rigid_cell_state(&mut self, slot: u32) {
        let generation = &mut self.rigid_cell_state_generations[slot as usize];
        *generation = generation.wrapping_add(1);
        self.rigid_cell_state_free.push(slot);
    }

    #[cfg(debug_assertions)]
    fn debug_assert_rigid_resident_invariants(&self) {
        debug_assert_eq!(
            self.rigid_cellular_contact_active.len(),
            self.rigid_cellular_bodies.len(),
            "rigid contact sidecar must stay positional with resident bodies"
        );
        debug_assert_eq!(
            self.rigid_granular_contact_active.len(),
            self.rigid_cellular_bodies.len(),
            "rigid granular sidecar must stay positional with resident bodies"
        );
        let mut ids = HashSet::with_capacity(self.rigid_cellular_bodies.len());
        let mut slots = HashSet::new();
        for body in &self.rigid_cellular_bodies {
            debug_assert!(
                body.id != 0,
                "resident rigid body has no persistent identity"
            );
            debug_assert!(
                ids.insert(body.id),
                "resident rigid identities must be unique"
            );
            for cell in &body.cells {
                debug_assert!((cell.state_slot as usize) < self.rigid_cell_state_generations.len());
                debug_assert_eq!(
                    self.rigid_cell_state_generations[cell.state_slot as usize],
                    cell.state_generation,
                    "resident rigid cell owns a stale state generation"
                );
                debug_assert!(
                    slots.insert(cell.state_slot),
                    "rigid state slot is owned twice"
                );
            }
        }
    }

    #[cfg(not(debug_assertions))]
    #[inline(always)]
    fn debug_assert_rigid_resident_invariants(&self) {}

    /// Validates and inserts one direct, non-canonical rigid cellular body.
    fn insert_authored_rigid_cellular_body(&mut self, placements: Vec<SceneEditCellPlacement>) {
        let mut unique: BTreeMap<(i32, i32), SceneEditCellPlacement> = BTreeMap::new();
        for placement in placements {
            unique.insert(
                (placement.coordinates.x, placement.coordinates.y),
                placement,
            );
        }
        if unique.is_empty()
            || !unique.values().all(|cell| {
                matches!(
                    self.data.materials().get(cell.material_identifier),
                    Some(Material::CellularStatic { .. })
                )
            })
        {
            return;
        }
        let used: usize = self
            .rigid_cellular_bodies
            .iter()
            .map(|body| body.cells.len())
            .sum();
        if used + unique.len() > self.cellular_physics_body_proxy.rigid_cell_capacity() {
            return;
        }
        let min_x = unique.keys().map(|(x, _)| *x).min().unwrap();
        let min_y = unique.keys().map(|(_, y)| *y).min().unwrap();
        let cells: Vec<_> = unique
            .into_values()
            .map(|cell| RigidCellularBodyCell {
                local: [cell.coordinates.x - min_x, cell.coordinates.y - min_y],
                material: cell.material_identifier,
                appearance: cell.appearance,
                state_slot: u32::MAX,
                state_generation: 0,
            })
            .collect();
        if cells.len() < self.rigid_component_minimum(&cells) {
            let debris = cells
                .into_iter()
                .filter_map(|cell| match self.data.materials().get(cell.material) {
                    Some(Material::CellularStatic {
                        debris_material: Some(material_identifier),
                        debris_yield_rate,
                        ..
                    }) if (((cell.local[0].wrapping_mul(31).wrapping_add(cell.local[1])) as u32
                        % 10_000) as f32)
                        < *debris_yield_rate * 10_000.0 =>
                    {
                        Some(SceneEditCellPlacement {
                            coordinates: CellCoordinates {
                                x: min_x + cell.local[0],
                                y: min_y + cell.local[1],
                            },
                            material_identifier: *material_identifier,
                            appearance: cell.appearance,
                        })
                    }
                    _ => None,
                })
                .collect();
            self.pending_runtime_edits.place_cells(debris);
            return;
        }
        let (friction, restitution) = self.rigid_cellular_material_response(&cells);
        self.insert_rigid_cellular_body(
            [min_x as f32 / 8.0, min_y as f32 / 8.0],
            cells,
            friction,
            restitution,
            None,
        );
    }

    /// Centralizes topology invalidation for every rigid-body insertion.
    fn insert_rigid_cellular_body(
        &mut self,
        position: [f32; 2],
        mut cells: Vec<RigidCellularBodyCell>,
        friction: f32,
        restitution: f32,
        inherited_state: Option<&[(f32, f32, f32)]>,
    ) {
        let mut uploads = Vec::new();
        for (index, cell) in cells.iter_mut().enumerate() {
            if cell.state_slot != u32::MAX {
                continue;
            }
            let slot = self
                .rigid_cell_state_free
                .pop()
                .expect("rigid cell capacity checked");
            cell.state_slot = slot;
            cell.state_generation = self.rigid_cell_state_generations[slot as usize];
            let integrity = match self.data.materials().get(cell.material) {
                Some(Material::CellularStatic {
                    default_integrity, ..
                }) => *default_integrity,
                _ => 0.0,
            };
            let temperature = self.initial_temperature(cell.material);
            let (integrity, amount, temperature) = inherited_state
                .and_then(|state| state.get(index).copied())
                .unwrap_or((integrity, 1.0, temperature));
            uploads.push([
                slot,
                integrity.to_bits(),
                amount.to_bits(),
                temperature.to_bits(),
            ]);
        }
        self.rigid_cell_state_upload
            .apply(self.accelerator.as_ref(), &uploads);
        let mut body = self.physics_world.insert_rigid_cellular_body(
            position,
            0.0,
            self.data.materials(),
            cells,
            friction,
            restitution,
            [0.0; 2],
            0.0,
        );
        body.id = self.next_rigid_cellular_body_id();
        self.rigid_cellular_bodies.push(body);
        self.rigid_cellular_topology_revision =
            self.rigid_cellular_topology_revision.wrapping_add(1);
        self.rigid_cellular_support.clear();
        self.rigid_cellular_recovery.clear();
        self.rigid_cellular_contact_active
            .resize(self.rigid_cellular_bodies.len(), false);
        self.rigid_granular_contact_active
            .resize(self.rigid_cellular_bodies.len(), false);
        self.debug_assert_rigid_resident_invariants();
    }

    fn next_rigid_cellular_body_id(&mut self) -> u64 {
        let id = self.rigid_cellular_body_id_next;
        self.rigid_cellular_body_id_next = self
            .rigid_cellular_body_id_next
            .checked_add(1)
            .expect("rigid body identity exhausted");
        id
    }

    /// Resolves one world cell to a resident physical Accelerator cell
    fn cell_edit_index(&self, coordinates: CellCoordinates) -> Option<usize> {
        let tile_coordinates: TileCoordinates = coordinates.tile_coordinates();
        let tile: Tile = self.tile_at(tile_coordinates)?;
        let [x, y]: [usize; 2] = coordinates.local_tile_coordinates();
        if !matches!(
            self.chunks.get(&tile_coordinates.chunk_coordinates()),
            Some(ChunkEntry::Active { chunk, .. }) if chunk.get_tile(tile_coordinates).is_ok()
        ) {
            return None;
        }
        Some(tile.0 as usize * 64 + y * 8 + x)
    }

    /// Writes final contiguous cellular edits to the two authoritative Accelerator buffers
    fn write_cell_edits(
        &self,
        edits: &[(
            usize,
            CellCoordinates,
            MaterialIdentifier,
            CellularAppearance,
            f32,
        )],
    ) {
        let mut start: usize = 0;
        while start < edits.len() {
            let mut end: usize = start + 1;
            while end < edits.len() && edits[end].0 == edits[end - 1].0 + 1 {
                end += 1;
            }
            let mut material_identifiers: Vec<u8> = Vec::with_capacity((end - start) * 4);
            let mut appearances: Vec<u8> = Vec::with_capacity((end - start) * 4);
            let mut integrities: Vec<u8> = Vec::with_capacity((end - start) * 4);
            let mut amounts: Vec<u8> = Vec::with_capacity((end - start) * 4);
            let mut temperatures: Vec<u8> = Vec::with_capacity((end - start) * 4);
            for edit in &edits[start..end] {
                material_identifiers.extend_from_slice(&edit.2.as_u32().to_le_bytes());
                appearances.extend_from_slice(&edit.3.0.to_le_bytes());
                integrities.extend_from_slice(&edit.4.to_bits().to_le_bytes());
                let (amount, temperature): (f32, f32) = if edit.2 == MaterialIdentifier::NULL {
                    (0.0, 0.0)
                } else {
                    (1.0, self.initial_temperature(edit.2))
                };
                amounts.extend_from_slice(&amount.to_bits().to_le_bytes());
                temperatures.extend_from_slice(&temperature.to_bits().to_le_bytes());
            }
            let offset: u64 = edits[start].0 as u64 * 4;
            self.accelerator.wgpu_queue().write_buffer(
                self.cellular_material_identifiers.wgpu_buffer(),
                offset,
                &material_identifiers,
            );
            self.accelerator.wgpu_queue().write_buffer(
                self.cellular_appearances.wgpu_buffer(),
                offset,
                &appearances,
            );
            self.accelerator.wgpu_queue().write_buffer(
                self.cellular_integrities.wgpu_buffer(),
                offset,
                &integrities,
            );
            self.accelerator.wgpu_queue().write_buffer(
                self.cellular_amounts.wgpu_buffer(),
                offset,
                &amounts,
            );
            self.accelerator.wgpu_queue().write_buffer(
                self.cellular_temperatures.wgpu_buffer(),
                offset,
                &temperatures,
            );
            self.cellular_dynamic.clear_cellular_dynamic_kinematics(
                self.accelerator.as_ref(),
                edits[start].0,
                end - start,
            );
            self.cellular_pressure.clear_transient_state(
                self.accelerator.as_ref(),
                edits[start].0,
                end - start,
            );
            start = end;
        }
    }

    fn initial_temperature(&self, material_identifier: MaterialIdentifier) -> f32 {
        self.data
            .materials()
            .thermal_properties(material_identifier)
            .and_then(|properties| properties.default_temperature)
            .unwrap_or(self.ambient_temperature)
    }

    pub(crate) const fn rigid_cell_amounts_buffer(&self) -> &AcceleratorBuffer {
        &self.rigid_cell_amounts
    }

    pub(crate) const fn rigid_cell_temperatures_buffer(&self) -> &AcceleratorBuffer {
        &self.rigid_cell_temperatures
    }

    /// Sets the automatic active-area target around a world position
    fn follow_position(&mut self, position: ScenePosition) {
        self.origin_target = TileCoordinates {
            x: position.tile_coordinates.x - i32::from(self.simulation_width) / 2,
            y: position.tile_coordinates.y - i32::from(self.simulation_height) / 2,
        };
    }

    /// Returns the exact tile area currently being simulated
    const fn area_active(&self) -> TileArea {
        TileArea::new(self.origin, self.simulation_width, self.simulation_height)
    }

    /// Returns the moving-fluid area including one camera-streaming batch outside the viewport
    fn area_fluid_active(&self) -> TileArea {
        let padding: u16 = u16::from(self.tile_streaming_batch_size);
        self.area_active()
            .expanded(padding, padding, padding, padding)
    }

    /// Returns the tile area resident on the Accelerator
    fn area_buffered(&self) -> TileArea {
        let buffer_size: i32 = i32::from(self.simulation_buffer_size);
        let dimensions: u16 = u16::from(self.simulation_buffer_size) * 2;
        TileArea::new(
            TileCoordinates {
                x: self.origin.x - buffer_size,
                y: self.origin.y - buffer_size,
            },
            self.simulation_width + dimensions,
            self.simulation_height + dimensions,
        )
    }

    /// Returns the chunk-aligned area required by the current Accelerator buffer
    fn area_streaming(&self) -> TileArea {
        self.area_buffered().chunk_area()
    }

    /// Returns the current chunk area plus one chunk in each movement direction
    fn area_prefetching(&self) -> TileArea {
        let velocity: Option<SceneVelocity> = self
            .possessed_actor()
            .and_then(|actor| self.actor_registry.get_velocity(actor))
            .copied();
        let left: bool = self.origin_target.x < self.origin.x
            || velocity.is_some_and(|velocity| velocity.x < 0.0);
        let bottom: bool = self.origin_target.y < self.origin.y
            || velocity.is_some_and(|velocity| velocity.y < 0.0);
        let right: bool = self.origin_target.x > self.origin.x
            || velocity.is_some_and(|velocity| velocity.x > 0.0);
        let top: bool = self.origin_target.y > self.origin.y
            || velocity.is_some_and(|velocity| velocity.y > 0.0);
        self.area_streaming().expanded(
            if left { Chunk::WIDTH } else { 0 },
            if bottom { Chunk::WIDTH } else { 0 },
            if right { Chunk::WIDTH } else { 0 },
            if top { Chunk::WIDTH } else { 0 },
        )
    }

    /// Requests every chunk in a chunk-aligned streaming area
    fn chunks_fetch(&mut self, streaming_area: TileArea) -> Result<(), io::Error> {
        for coordinates in streaming_area.iterate_chunk_coordinates() {
            match self.chunks.get(&coordinates) {
                None | Some(ChunkEntry::Error(_)) => self.chunk_load(coordinates)?,
                Some(ChunkEntry::Active { .. })
                | Some(ChunkEntry::Loading { .. })
                | Some(ChunkEntry::Generating { .. })
                | Some(ChunkEntry::Saving { .. }) => {}
            };
        }
        Ok(())
    }

    /// Reads an unloaded chunk into this `Scene`
    fn chunk_load(&mut self, coordinates: TileCoordinates) -> Result<(), io::Error> {
        // avoid duplicate work
        if let Some(entry) = self.chunks.get(&coordinates) {
            match entry {
                ChunkEntry::Active { .. }
                | ChunkEntry::Loading { .. }
                | ChunkEntry::Generating { .. }
                | ChunkEntry::Saving { .. } => return Ok(()),
                _ => (),
            }
        }
        let streaming_identifier: u64 = self.chunks_streaming_identifier_next;
        self.chunks_streaming_identifier_next =
            self.chunks_streaming_identifier_next.wrapping_add(1);
        self.chunks.insert(
            coordinates,
            ChunkEntry::Loading {
                streaming_identifier,
            },
        );
        let data: SceneData = self.data.clone();
        let sender: SyncSender<ChunkStreamingResponse> =
            self.chunk_streaming_response_sender.clone();
        // spawn new thread to attempt read
        std::thread::spawn(move || {
            let result: Result<Option<Box<Chunk>>, Box<dyn Error + Send + Sync>> = data
                .read_chunk(coordinates)
                .map(|chunk| chunk.map(Box::new))
                .map_err(|error| Box::new(error).into());
            sender
                .send(ChunkStreamingResponse::Loaded {
                    streaming_identifier,
                    coordinates,
                    result,
                })
                .unwrap();
        });
        Ok(())
    }

    /// Generates a new chunk for this `Scene`
    fn chunk_generate(&mut self, coordinates: TileCoordinates) -> Result<(), io::Error> {
        // avoid duplicate work
        if let Some(entry) = self.chunks.get(&coordinates) {
            match entry {
                ChunkEntry::Active { .. }
                | ChunkEntry::Generating { .. }
                | ChunkEntry::Saving { .. } => return Ok(()),
                _ => (),
            }
        }
        let streaming_identifier: u64 = self.chunks_streaming_identifier_next;
        self.chunks_streaming_identifier_next =
            self.chunks_streaming_identifier_next.wrapping_add(1);
        self.chunks.insert(
            coordinates,
            ChunkEntry::Generating {
                streaming_identifier,
            },
        );
        let generator: Arc<dyn SceneGenerator> = self.generator.clone();
        let sender: SyncSender<ChunkStreamingResponse> =
            self.chunk_streaming_response_sender.clone();
        // spawn new thread for generation
        std::thread::spawn(move || {
            let result: Result<Box<Chunk>, Box<dyn Error + Send + Sync>> =
                Ok(Box::new(generator.generate_chunk(coordinates)));
            sender
                .send(ChunkStreamingResponse::Generated {
                    streaming_identifier,
                    coordinates,
                    result,
                })
                .unwrap();
        });
        Ok(())
    }

    /// Saves dirty chunks and removes entries outside the one-chunk retention region
    fn chunks_save(&mut self) -> Result<(), io::Error> {
        let retention_area: TileArea =
            self.area_streaming()
                .expanded(Chunk::WIDTH, Chunk::WIDTH, Chunk::WIDTH, Chunk::WIDTH);
        let coordinates: Vec<TileCoordinates> = self
            .chunks
            .iter()
            .filter_map(|(coordinates, entry)| {
                if retention_area.contains(*coordinates)
                    || matches!(
                        entry,
                        ChunkEntry::Loading { .. }
                            | ChunkEntry::Generating { .. }
                            | ChunkEntry::Saving { .. },
                    )
                {
                    None
                } else {
                    Some(*coordinates)
                }
            })
            .collect();
        for coordinates in coordinates {
            // keep stale CPU chunks unavailable to save or removal until downloads are applied
            if self.tile_download_pending_for_chunk(coordinates)?
                || self.fluid_transfer_pending_for_chunk(coordinates)?
                || self.gas_transfer_pending_for_chunk(coordinates)?
            {
                continue;
            }
            let Some(entry) = self.chunks.remove(&coordinates) else {
                continue;
            };
            let ChunkEntry::Active {
                chunk,
                is_dirty: true,
            } = entry
            else {
                continue;
            };
            let streaming_identifier: u64 = self.chunks_streaming_identifier_next;
            self.chunks_streaming_identifier_next =
                self.chunks_streaming_identifier_next.wrapping_add(1);
            self.chunks.insert(
                coordinates,
                ChunkEntry::Saving {
                    streaming_identifier,
                },
            );
            let data: SceneData = self.data.clone();
            let sender: SyncSender<ChunkStreamingResponse> =
                self.chunk_streaming_response_sender.clone();
            std::thread::spawn(move || {
                let result: Result<Box<Chunk>, (Box<Chunk>, io::Error)> =
                    match data.write_chunk(&chunk) {
                        Ok(()) => Ok(Box::new(chunk)),
                        Err(error) => Err((Box::new(chunk), error)),
                    };
                sender
                    .send(ChunkStreamingResponse::Saved {
                        streaming_identifier,
                        coordinates,
                        result,
                    })
                    .unwrap();
            });
        }
        Ok(())
    }

    /// Refreshes streamed chunks and queues newly available resident tiles for upload
    fn chunks_refresh(&mut self) -> Result<(), io::Error> {
        // apply completed background loads and generations before planning movement
        let mut chunks_available: Vec<TileCoordinates> = Vec::new();
        while let Ok(response) = self.chunk_streaming_responses.try_recv() {
            match response {
                ChunkStreamingResponse::Loaded {
                    streaming_identifier,
                    coordinates,
                    result,
                } => {
                    let matches_request = matches!( // ignore mutated entries
                        self.chunks.get(&coordinates),
                        Some(ChunkEntry::Loading { streaming_identifier: current })
                            if *current == streaming_identifier
                    );
                    if !matches_request {
                        continue;
                    }
                    match result {
                        // missing chunk begins a separate generation operation
                        Ok(Some(chunk)) => {
                            let mut chunk = *chunk;
                            chunk.resolve_uninitialized_temperatures(|identifier| {
                                self.initial_temperature(identifier)
                            });
                            self.chunks.insert(
                                coordinates,
                                ChunkEntry::Active {
                                    chunk,
                                    is_dirty: false,
                                },
                            );
                            chunks_available.push(coordinates);
                        }
                        Ok(None) => self.chunk_generate(coordinates)?,
                        Err(error) => {
                            let error: Box<dyn Error> = error;
                            self.chunks.insert(coordinates, ChunkEntry::Error(error));
                        }
                    }
                }
                ChunkStreamingResponse::Generated {
                    streaming_identifier,
                    coordinates,
                    result,
                } => {
                    if !matches!( // ignore mutated entries
                        self.chunks.get(&coordinates),
                        Some(ChunkEntry::Generating { streaming_identifier: current })
                            if *current == streaming_identifier
                    ) {
                        continue;
                    }
                    match result {
                        // retain the successfully generated chunk
                        Ok(chunk) => {
                            let mut chunk = *chunk;
                            chunk.resolve_uninitialized_temperatures(|identifier| {
                                self.initial_temperature(identifier)
                            });
                            self.chunks.insert(
                                coordinates,
                                ChunkEntry::Active {
                                    chunk,
                                    is_dirty: false,
                                },
                            );
                            chunks_available.push(coordinates);
                        }
                        Err(error) => {
                            let error: Box<dyn Error> = error;
                            self.chunks.insert(coordinates, ChunkEntry::Error(error));
                        }
                    }
                }
                ChunkStreamingResponse::Saved {
                    streaming_identifier,
                    coordinates,
                    result,
                } => {
                    if !matches!(
                        self.chunks.get(&coordinates),
                        Some(ChunkEntry::Saving { streaming_identifier: current })
                            if *current == streaming_identifier
                    ) {
                        continue;
                    }
                    match result {
                        Ok(chunk) => {
                            if self.area_prefetching().contains(coordinates) {
                                self.chunks.insert(
                                    coordinates,
                                    ChunkEntry::Active {
                                        chunk: *chunk,
                                        is_dirty: false,
                                    },
                                );
                                chunks_available.push(coordinates);
                            } else {
                                self.chunks.remove(&coordinates);
                            }
                        }
                        Err((chunk, error)) => {
                            self.chunks.insert(
                                coordinates,
                                ChunkEntry::Active {
                                    chunk: *chunk,
                                    is_dirty: true,
                                },
                            );
                            return Err(error);
                        }
                    }
                }
            }
        }
        self.chunks_fetch(self.area_prefetching())?;
        // move by at most one batch
        let batch_size: i64 = self.tile_streaming_batch_size as i64;
        let x_difference: i64 = self.origin_target.x as i64 - self.origin.x as i64;
        let y_difference: i64 = self.origin_target.y as i64 - self.origin.y as i64;
        if x_difference >= batch_size {
            self.shift_right()?;
        } else if x_difference <= -batch_size {
            self.shift_left()?;
        } else if y_difference >= batch_size {
            self.shift_up()?;
        } else if y_difference <= -batch_size {
            self.shift_down()?;
        }
        self.chunks_save()?;
        // queue upload only newly available chunks
        for coordinates in chunks_available {
            self.rigid_owner_load(coordinates);
            let _ = self.tiles_upload(TileArea::new(coordinates, Chunk::WIDTH, Chunk::WIDTH));
        }
        Ok(())
    }

    /// Shifts the active tile area up by the configured streaming batch size
    fn shift_up(&mut self) -> Result<(), io::Error> {
        let batch_size: i32 = self.tile_streaming_batch_size as i32;
        let mut origin: TileCoordinates = self.origin;
        origin.y += batch_size;
        self.shift_to(origin)
    }

    /// Shifts the active tile area down by the configured streaming batch size
    fn shift_down(&mut self) -> Result<(), io::Error> {
        let batch_size: i32 = self.tile_streaming_batch_size as i32;
        let mut origin: TileCoordinates = self.origin;
        origin.y -= batch_size;
        self.shift_to(origin)
    }

    /// Shifts the active tile area right by the configured streaming batch size
    fn shift_right(&mut self) -> Result<(), io::Error> {
        let batch_size: i32 = self.tile_streaming_batch_size as i32;
        let mut origin: TileCoordinates = self.origin;
        origin.x += batch_size;
        self.shift_to(origin)
    }

    /// Shifts the active tile area left by the configured streaming batch size
    fn shift_left(&mut self) -> Result<(), io::Error> {
        let batch_size: i32 = self.tile_streaming_batch_size as i32;
        let mut origin: TileCoordinates = self.origin;
        origin.x -= batch_size;
        self.shift_to(origin)
    }

    /// Moves the origin, remaps ring slots, and streams tiles
    fn shift_to(&mut self, new_origin: TileCoordinates) -> Result<(), io::Error> {
        // verify the incoming CPU state before reserving outgoing Accelerator state
        let buffer_size: i32 = i32::from(self.simulation_buffer_size);
        let dimensions: u16 = u16::from(self.simulation_buffer_size) * 2;
        let width: u16 = self.simulation_width + dimensions;
        let height: u16 = self.simulation_height + dimensions;
        let buffered_area: TileArea = TileArea::new(
            TileCoordinates {
                x: new_origin.x - buffer_size,
                y: new_origin.y - buffer_size,
            },
            width,
            height,
        );
        let streaming_area: TileArea = buffered_area.chunk_area();
        self.rigid_desired_owners = streaming_area
            .expanded(Chunk::WIDTH, Chunk::WIDTH, Chunk::WIDTH, Chunk::WIDTH)
            .chunk_area()
            .iterate_chunk_coordinates()
            .collect();
        self.chunks_fetch(streaming_area)?;
        for owner in streaming_area
            .expanded(Chunk::WIDTH, Chunk::WIDTH, Chunk::WIDTH, Chunk::WIDTH)
            .chunk_area()
            .iterate_chunk_coordinates()
        {
            self.rigid_owner_load(owner);
        }
        if !streaming_area
            .iterate_chunk_coordinates()
            .all(|coordinates| {
                matches!(
                    self.chunks.get(&coordinates),
                    Some(ChunkEntry::Active { .. })
                )
            })
        {
            return Ok(());
        }
        if !streaming_area.iterate_chunk_coordinates().all(|owner| {
            matches!(
                self.rigid_owner_loads.get(&owner),
                Some(RigidOwnerLoad::Ready(_))
            )
        }) {
            return Ok(());
        }
        // Capture under the old terrain interpretation.  If its bounded
        // staging frontier is full, retain the current valid area for a later
        // frame rather than letting bodies outrun their support.
        if !self.rigid_dormancy_begin(buffered_area)? {
            return Ok(());
        }
        let batch_size: u16 = u16::from(self.tile_streaming_batch_size);
        let old_buffered_origin: TileCoordinates = TileCoordinates {
            x: self.origin.x - buffer_size,
            y: self.origin.y - buffer_size,
        };
        let tiles_download_area: TileArea;
        let tiles_upload_area: TileArea;
        if new_origin.x > self.origin.x {
            tiles_download_area = TileArea::new(old_buffered_origin, batch_size, height);
            tiles_upload_area = TileArea::new(
                TileCoordinates {
                    x: new_origin.x - buffer_size + width as i32 - batch_size as i32,
                    y: new_origin.y - buffer_size,
                },
                batch_size,
                height,
            );
        } else if new_origin.x < self.origin.x {
            tiles_download_area = TileArea::new(
                TileCoordinates {
                    x: old_buffered_origin.x + width as i32 - batch_size as i32,
                    y: old_buffered_origin.y,
                },
                batch_size,
                height,
            );
            tiles_upload_area = TileArea::new(
                TileCoordinates {
                    x: new_origin.x - buffer_size,
                    y: new_origin.y - buffer_size,
                },
                batch_size,
                height,
            );
        } else if new_origin.y > self.origin.y {
            tiles_download_area = TileArea::new(old_buffered_origin, width, batch_size);
            tiles_upload_area = TileArea::new(
                TileCoordinates {
                    x: new_origin.x - buffer_size,
                    y: new_origin.y - buffer_size + height as i32 - batch_size as i32,
                },
                width,
                batch_size,
            );
        } else if new_origin.y < self.origin.y {
            tiles_download_area = TileArea::new(
                TileCoordinates {
                    x: old_buffered_origin.x,
                    y: old_buffered_origin.y + height as i32 - batch_size as i32,
                },
                width,
                batch_size,
            );
            tiles_upload_area = TileArea::new(
                TileCoordinates {
                    x: new_origin.x - buffer_size,
                    y: new_origin.y - buffer_size,
                },
                width,
                batch_size,
            );
        } else {
            return Ok(());
        }

        // defer rapid re-entry until the prior download has reached its CPU chunk
        if self.tile_download_pending_in(tiles_upload_area)?
            || self.fluid_download_pending_in(tiles_upload_area)?
            || self.fluid_upload_pending_in(tiles_download_area)?
            || self.gas_download_pending_in(tiles_upload_area)?
        {
            return Ok(());
        }

        // materialize queued CPU state before capturing the old physical slots
        self.tile_uploads_submit()?;
        self.fluid_uploads_submit()?;

        // capture and submit old physical slots before changing their world interpretation
        self.tile_downloads_queue(tiles_download_area)?;
        self.tile_downloads_submit()?;
        self.fluid_downloads_queue(tiles_download_area);
        self.fluid_downloads_submit()?;
        self.gas_downloads_queue(tiles_download_area);
        self.gas_downloads_submit()?;

        // remap only the reused edge; retained tiles keep their physical kinematic slots
        if new_origin.x > self.origin.x {
            self.tiles_ring_offset_x = (self.tiles_ring_offset_x + batch_size) % width;
        } else if new_origin.x < self.origin.x {
            self.tiles_ring_offset_x = (self.tiles_ring_offset_x + width - batch_size) % width;
        } else if new_origin.y > self.origin.y {
            self.tiles_ring_offset_y = (self.tiles_ring_offset_y + batch_size) % height;
        } else {
            self.tiles_ring_offset_y = (self.tiles_ring_offset_y + height - batch_size) % height;
        }
        self.origin = new_origin;
        self.cellular_collision_dirty = true;
        self.fluids.refresh(
            self.accelerator.as_ref(),
            self.area_fluid_active().origin(),
            self.area_fluid_active().dimensions()[0],
            self.area_fluid_active().dimensions()[1],
            TileCoordinates {
                x: new_origin.x - buffer_size,
                y: new_origin.y - buffer_size,
            },
            width,
            height,
            self.tiles_ring_offset_x,
            self.tiles_ring_offset_y,
        );
        self.gas_clear_area(tiles_upload_area);
        let _ = self.tiles_upload(tiles_upload_area);
        self.fluid_uploads_queue(tiles_upload_area)?;
        self.gas_upload_area(tiles_upload_area)?;
        Ok(())
    }

    /// Returns the active tile at a provided `TileCoordinates` if one exists
    pub fn tile_at(&self, coordinates: TileCoordinates) -> Option<Tile> {
        let buffer_size: i32 = i32::from(self.simulation_buffer_size);
        let width: usize = self.simulation_width as usize + buffer_size as usize * 2;
        let x: usize = usize::try_from(coordinates.x - (self.origin.x - buffer_size)).ok()?;
        let y: usize = usize::try_from(coordinates.y - (self.origin.y - buffer_size)).ok()?;
        let height: usize = self.simulation_height as usize + buffer_size as usize * 2;
        if x >= width || y >= height {
            return None;
        }
        let x: usize = (x + self.tiles_ring_offset_x as usize) % width;
        let y: usize = (y + self.tiles_ring_offset_y as usize) % height;
        self.tiles.get(y * width + x).copied()
    }

    /// Queues one dense gas strip export under the current ring interpretation
    fn gas_downloads_queue(&mut self, area: TileArea) {
        if self.gases.gas_count() == 0 {
            return;
        }
        let dimensions: u16 = (self.simulation_width + u16::from(self.simulation_buffer_size) * 2)
            .max(self.simulation_height + u16::from(self.simulation_buffer_size) * 2);
        let maximum_cell_count: u32 =
            u32::from(self.tile_streaming_batch_size) * u32::from(dimensions) * 64;
        let download: Arc<Mutex<GasDownload>> = self.gas_download_pool.pop().unwrap_or_else(|| {
            Arc::new(Mutex::new(GasDownload::new(
                self.accelerator.as_ref(),
                area,
                maximum_cell_count,
                self.gases.gas_count(),
            )))
        });
        download.lock().unwrap().reset(area);
        self.gas_downloads.push(download);
    }

    /// Returns whether an incoming area overlaps unresolved exported gas
    fn gas_download_pending_in(&self, area: TileArea) -> Result<bool, io::Error> {
        for download in &self.gas_downloads {
            if download
                .lock()
                .map_err(|_| io::Error::other("Gas download is unavailable"))?
                .area
                .intersects(area)
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Returns whether a chunk is pinned by an unresolved gas export
    fn gas_transfer_pending_for_chunk(
        &self,
        coordinates: TileCoordinates,
    ) -> Result<bool, io::Error> {
        for download in &self.gas_downloads {
            if download
                .lock()
                .map_err(|_| io::Error::other("Gas download is unavailable"))?
                .area
                .chunk_area()
                .contains(coordinates)
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Clears a world area through its current physical ring mapping
    fn gas_clear_area(&self, area: TileArea) {
        let buffered_area: TileArea = self.area_buffered();
        let dimensions: [u16; 2] = buffered_area.dimensions();
        self.gases.clear_area(
            self.accelerator.as_ref(),
            area,
            buffered_area.origin(),
            dimensions[0],
            dimensions[1],
            self.tiles_ring_offset_x,
            self.tiles_ring_offset_y,
        );
    }

    /// Moves dormant sparse gas from CPU chunks into dense resident Accelerator fields
    fn gas_upload_area(&mut self, area: TileArea) -> Result<(), io::Error> {
        if self.gases.gas_count() == 0 {
            return Ok(());
        }
        let chunk_coordinates: Vec<TileCoordinates> =
            area.chunk_area().iterate_chunk_coordinates().collect();
        for coordinates in &chunk_coordinates {
            if !matches!(
                self.chunks.get(coordinates),
                Some(ChunkEntry::Active { .. })
            ) {
                return Err(io::Error::other("Incoming gas chunk is not active"));
            }
        }
        let mut cells: Vec<ChunkGasCell> = Vec::new();
        for coordinates in chunk_coordinates {
            let Some(ChunkEntry::Active { chunk, is_dirty }) = self.chunks.get_mut(&coordinates)
            else {
                unreachable!();
            };
            let mut chunk_cells: Vec<ChunkGasCell> = chunk.take_dormant_gas_cells(area);
            if !chunk_cells.is_empty() {
                *is_dirty = true;
            }
            cells.append(&mut chunk_cells);
        }
        if cells.is_empty() {
            return Ok(());
        }
        let mut cells = cells;
        for cell in &mut cells {
            if !cell.temperature.is_finite() {
                cell.temperature = self.ambient_temperature;
            }
        }
        let upload: GasUpload = GasUpload::new(area, cells);
        if let Err(error) = upload.validate(self.data.materials()) {
            self.gas_cells_restore(upload.cells)?;
            return Err(error);
        }
        let physical_indices: Option<Vec<usize>> = upload
            .cells
            .iter()
            .map(|cell| self.cell_edit_index(cell.coordinates))
            .collect();
        let Some(physical_indices) = physical_indices else {
            self.gas_cells_restore(upload.cells)?;
            return Err(io::Error::other(
                "Incoming dormant gas cell is outside Accelerator residency",
            ));
        };
        self.gases
            .import(self.accelerator.as_ref(), &upload, &physical_indices);
        Ok(())
    }

    /// Returns sparse gas cells to their owning CPU chunks after a failed import validation
    fn gas_cells_restore(&mut self, cells: Vec<ChunkGasCell>) -> Result<(), io::Error> {
        for cell in cells {
            let coordinates: TileCoordinates = cell.tile_coordinates().chunk_coordinates();
            let Some(ChunkEntry::Active { chunk, is_dirty }) = self.chunks.get_mut(&coordinates)
            else {
                return Err(io::Error::other("Dormant gas source chunk is not active"));
            };
            chunk
                .insert_dormant_gas_cell(cell)
                .map_err(|_| io::Error::other("Dormant gas cell is outside its source chunk"))?;
            *is_dirty = true;
        }
        Ok(())
    }

    /// Applies completed gas exports to their world-position CPU chunks
    fn gas_downloads_apply_completed(&mut self) -> Result<(), io::Error> {
        let mut index: usize = 0;
        while index < self.gas_downloads.len() {
            let mut download = self.gas_downloads[index]
                .lock()
                .map_err(|_| io::Error::other("Gas download is unavailable"))?;
            let Some(result) = download.result.as_ref() else {
                index += 1;
                continue;
            };
            if let Err(error) = result {
                return Err(io::Error::other(format!("Gas download failed: {error}")));
            }
            let area: TileArea = download.area;
            for cell in result.as_ref().unwrap() {
                let coordinates: TileCoordinates = cell.tile_coordinates();
                if !area.contains(coordinates)
                    || !matches!(
                        self.chunks.get(&coordinates.chunk_coordinates()),
                        Some(ChunkEntry::Active { .. }),
                    )
                {
                    return Err(io::Error::other(
                        "Exported gas cell has no active destination chunk",
                    ));
                }
            }
            let cells: Vec<ChunkGasCell> = download.result.take().unwrap().unwrap();
            drop(download);
            self.gas_cells_restore(cells)?;
            let download: Arc<Mutex<GasDownload>> = self.gas_downloads.swap_remove(index);
            self.gas_download_pool.push(download);
        }
        Ok(())
    }

    /// Submits queued gas exports and begins their asynchronous readbacks
    fn gas_downloads_submit(&self) -> Result<(), io::Error> {
        for download in &self.gas_downloads {
            let mut state = download
                .lock()
                .map_err(|_| io::Error::other("Gas download is unavailable"))?;
            if state.is_started {
                continue;
            }
            let buffered_area: TileArea = self.area_buffered();
            let buffered_dimensions: [u16; 2] = buffered_area.dimensions();
            self.gases.export(
                self.accelerator.as_ref(),
                &state,
                buffered_area.origin(),
                buffered_dimensions[0],
                buffered_dimensions[1],
                self.tiles_ring_offset_x,
                self.tiles_ring_offset_y,
            );
            state.is_started = true;
            let area: TileArea = state.area;
            let dimensions: [u16; 2] = area.dimensions();
            let byte_count: usize = usize::from(dimensions[0])
                * usize::from(dimensions[1])
                * 64
                * (5 + self.gases.gas_count() as usize)
                * 4;
            let gas_identifiers: Vec<MaterialIdentifier> = self
                .data
                .materials()
                .iter()
                .filter_map(|(identifier, material)| {
                    matches!(material, Material::Gas { .. }).then_some(identifier)
                })
                .collect();
            let buffer: wgpu::Buffer = state.buffer.clone();
            let mapped_buffer: wgpu::Buffer = buffer.clone();
            let download: Arc<Mutex<GasDownload>> = download.clone();
            drop(state);
            buffer
                .slice(0..byte_count as u64)
                .map_async(wgpu::MapMode::Read, move |result| {
                    let bytes: Result<Vec<u8>, io::Error> = match result {
                        Ok(()) => {
                            match mapped_buffer.slice(0..byte_count as u64).get_mapped_range() {
                                Ok(mapped_data) => {
                                    let bytes: Vec<u8> = mapped_data.to_vec();
                                    drop(mapped_data);
                                    mapped_buffer.unmap();
                                    Ok(bytes)
                                }
                                Err(error) => {
                                    mapped_buffer.unmap();
                                    Err(io::Error::other(error.to_string()))
                                }
                            }
                        }
                        Err(_) => Err(io::Error::other("Gas download failed")),
                    };
                    std::thread::spawn(move || {
                        let result: Result<Vec<ChunkGasCell>, io::Error> =
                            bytes.and_then(|bytes| {
                                GasDownload::deserialize(&bytes, area, &gas_identifiers)
                            });
                        if let Ok(mut state) = download.lock() {
                            state.result = Some(result);
                        }
                    });
                });
        }
        Ok(())
    }

    /// Queues one authoritative fluid export under the current ring interpretation
    fn fluid_downloads_queue(&mut self, area: TileArea) {
        let download: Arc<Mutex<FluidDownload>> =
            self.fluid_download_pool.pop().unwrap_or_else(|| {
                Arc::new(Mutex::new(FluidDownload::new(
                    self.accelerator.as_ref(),
                    area,
                    self.fluids.particle_capacity(),
                )))
            });
        download.lock().unwrap().reset(area);
        self.fluid_downloads.push(download);
    }

    /// Returns whether an incoming area overlaps unresolved exported fluid
    fn fluid_download_pending_in(&self, area: TileArea) -> Result<bool, io::Error> {
        for download in &self.fluid_downloads {
            if download
                .lock()
                .map_err(|_| io::Error::other("Fluid download is unavailable"))?
                .area
                .intersects(area)
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Returns whether an outgoing area overlaps unresolved imported fluid
    fn fluid_upload_pending_in(&self, area: TileArea) -> Result<bool, io::Error> {
        for upload in &self.fluid_uploads {
            if upload
                .lock()
                .map_err(|_| io::Error::other("Fluid upload is unavailable"))?
                .area
                .intersects(area)
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Returns whether a chunk is pinned by an unresolved fluid ownership transfer
    fn fluid_transfer_pending_for_chunk(
        &self,
        coordinates: TileCoordinates,
    ) -> Result<bool, io::Error> {
        for download in &self.fluid_downloads {
            if download
                .lock()
                .map_err(|_| io::Error::other("Fluid download is unavailable"))?
                .area
                .chunk_area()
                .contains(coordinates)
            {
                return Ok(true);
            }
        }
        for upload in &self.fluid_uploads {
            if upload
                .lock()
                .map_err(|_| io::Error::other("Fluid upload is unavailable"))?
                .area
                .chunk_area()
                .contains(coordinates)
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Removes incoming dormant records from chunks into a pending Accelerator transfer
    fn fluid_uploads_queue(&mut self, area: TileArea) -> Result<(), io::Error> {
        let chunk_coordinates: Vec<TileCoordinates> =
            area.chunk_area().iterate_chunk_coordinates().collect();
        for coordinates in &chunk_coordinates {
            let Some(ChunkEntry::Active { .. }) = self.chunks.get(coordinates) else {
                return Err(io::Error::other("Incoming fluid chunk is not active"));
            };
        }
        let mut particles: Vec<ChunkFluidParticle> = Vec::new();
        for coordinates in chunk_coordinates {
            let Some(ChunkEntry::Active { chunk, is_dirty }) = self.chunks.get_mut(&coordinates)
            else {
                unreachable!();
            };
            let mut chunk_particles: Vec<ChunkFluidParticle> =
                chunk.take_dormant_fluid_particles(area);
            if !chunk_particles.is_empty() {
                *is_dirty = true;
            }
            particles.append(&mut chunk_particles);
        }
        if particles.is_empty() {
            return Ok(());
        }
        if particles.len() > self.fluids.particle_capacity() as usize {
            for particle in particles {
                let Some(ChunkEntry::Active { chunk, .. }) = self
                    .chunks
                    .get_mut(&particle.tile_coordinates().chunk_coordinates())
                else {
                    unreachable!();
                };
                chunk.insert_dormant_fluid_particle(particle).unwrap();
            }
            return Err(io::Error::other(
                "Incoming dormant fluid exceeds the Accelerator particle pool capacity",
            ));
        }
        for particle in &mut particles {
            if !particle.temperature.is_finite() {
                particle.temperature = self.initial_temperature(particle.material_identifier);
            }
        }
        if !particles.iter().all(|particle| {
            matches!(
                self.data.materials().get(particle.material_identifier),
                Some(Material::Fluid { .. }),
            )
        }) {
            for particle in particles {
                let Some(ChunkEntry::Active { chunk, .. }) = self
                    .chunks
                    .get_mut(&particle.tile_coordinates().chunk_coordinates())
                else {
                    unreachable!();
                };
                chunk.insert_dormant_fluid_particle(particle).unwrap();
            }
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Dormant particle references an unregistered fluid material",
            ));
        }
        self.fluid_uploads
            .push(Arc::new(Mutex::new(FluidUpload::new(
                self.accelerator.as_ref(),
                area,
                particles,
            ))));
        Ok(())
    }

    /// Applies completed fluid exports to their current-position CPU chunks
    fn fluid_downloads_apply_completed(&mut self) -> Result<(), io::Error> {
        let mut index: usize = 0;
        while index < self.fluid_downloads.len() {
            let mut download = self.fluid_downloads[index]
                .lock()
                .map_err(|_| io::Error::other("Fluid download is unavailable"))?;
            let Some(result) = download.result.as_ref() else {
                index += 1;
                continue;
            };
            if let Err(error) = result {
                return Err(io::Error::other(format!("Fluid download failed: {error}")));
            }
            let area: TileArea = download.area;
            for particle in result.as_ref().unwrap() {
                let coordinates: TileCoordinates = particle.tile_coordinates();
                if !area.contains(coordinates)
                    || !matches!(
                        self.chunks.get(&coordinates.chunk_coordinates()),
                        Some(ChunkEntry::Active { .. }),
                    )
                {
                    return Err(io::Error::other(
                        "Exported fluid particle has no active destination chunk",
                    ));
                }
            }
            let particles: Vec<ChunkFluidParticle> = download.result.take().unwrap().unwrap();
            drop(download);
            for particle in particles {
                let Some(ChunkEntry::Active { chunk, is_dirty }) = self
                    .chunks
                    .get_mut(&particle.tile_coordinates().chunk_coordinates())
                else {
                    unreachable!();
                };
                chunk.insert_dormant_fluid_particle(particle).map_err(|_| {
                    io::Error::other("Exported fluid particle is outside its destination chunk")
                })?;
                *is_dirty = true;
            }
            let download: Arc<Mutex<FluidDownload>> = self.fluid_downloads.swap_remove(index);
            self.fluid_download_pool.push(download);
        }
        Ok(())
    }

    /// Restores failed imports to CPU ownership and completes successful transfers
    fn fluid_uploads_apply_completed(&mut self) -> Result<(), io::Error> {
        let mut index: usize = 0;
        while index < self.fluid_uploads.len() {
            let mut upload = self.fluid_uploads[index]
                .lock()
                .map_err(|_| io::Error::other("Fluid upload is unavailable"))?;
            let Some(result) = upload.result.as_ref() else {
                index += 1;
                continue;
            };
            if let Err(error) = result {
                return Err(io::Error::other(format!("Fluid upload failed: {error}")));
            }
            for particle in result.as_ref().unwrap() {
                if !matches!(
                    self.chunks
                        .get(&particle.tile_coordinates().chunk_coordinates()),
                    Some(ChunkEntry::Active { .. }),
                ) {
                    return Err(io::Error::other(
                        "Rejected fluid particle has no active source chunk",
                    ));
                }
            }
            let failed: Vec<ChunkFluidParticle> = upload.result.take().unwrap().unwrap();
            drop(upload);
            for particle in &failed {
                let Some(ChunkEntry::Active { chunk, is_dirty }) = self
                    .chunks
                    .get_mut(&particle.tile_coordinates().chunk_coordinates())
                else {
                    unreachable!();
                };
                chunk
                    .insert_dormant_fluid_particle(*particle)
                    .map_err(|_| {
                        io::Error::other("Rejected fluid particle is outside its source chunk")
                    })?;
                *is_dirty = true;
            }
            self.fluid_uploads.swap_remove(index);
            if !failed.is_empty() {
                return Err(io::Error::other(format!(
                    "Accelerator fluid pool rejected {} dormant particles",
                    failed.len(),
                )));
            }
        }
        Ok(())
    }

    /// Applies a completed sample only to the actor for which it was dispatched
    fn fluid_sample_apply_completed(&mut self) -> Result<(), io::Error> {
        let Some(result) = self
            .fluid_sample_result
            .lock()
            .map_err(|_| io::Error::other("Pawn fluid sample result is unavailable"))?
            .take()
        else {
            return Ok(());
        };
        let actor: Actor = self
            .fluid_sample_actor
            .take()
            .ok_or_else(|| io::Error::other("Completed pawn fluid sample has no actor"))?;
        let sample: [f32; 5] = result.map_err(io::Error::other)?;
        if self.possessed_actor() == Some(actor) {
            self.actor_registry.apply_swimming_sample(actor, sample);
        }
        Ok(())
    }

    /// Dispatches one tiny derived-cell sample without waiting for its readback
    fn fluid_sample_submit(&mut self) -> Result<(), io::Error> {
        if self.fluid_sample_actor.is_some() {
            return Ok(());
        }
        let Some(actor) = self.possessed_actor() else {
            return Ok(());
        };
        let Some((center, shape)) = self.actor_registry.swimming_pawn_sample(actor) else {
            return Ok(());
        };
        let active_area: TileArea = self.area_fluid_active();
        let active_dimensions: [u16; 2] = active_area.dimensions();
        let buffered_area: TileArea = self.area_buffered();
        let buffered_dimensions: [u16; 2] = buffered_area.dimensions();
        self.fluids.sample_pawn(
            self.accelerator.as_ref(),
            &self.fluid_sample_buffer,
            center,
            shape,
            active_area.origin(),
            active_dimensions[0],
            active_dimensions[1],
            buffered_area.origin(),
            buffered_dimensions[0],
            buffered_dimensions[1],
            self.tiles_ring_offset_x,
            self.tiles_ring_offset_y,
            self.gravity,
        );
        self.fluid_sample_actor = Some(actor);
        let mapped_buffer: wgpu::Buffer = self.fluid_sample_buffer.clone();
        let result: Arc<Mutex<Option<Result<[f32; 5], String>>>> = self.fluid_sample_result.clone();
        self.fluid_sample_buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |mapping| {
                let sample: Result<[f32; 5], String> = mapping
                    .map_err(|_| "Pawn fluid sample readback failed".to_owned())
                    .and_then(|()| {
                        mapped_buffer
                            .slice(..)
                            .get_mapped_range()
                            .map_err(|error| error.to_string())
                            .and_then(|mapped| {
                                let sample: [f32; 5] = std::array::from_fn(|index| {
                                    f32::from_bits(u32::from_le_bytes(
                                        mapped[index * 4..index * 4 + 4].try_into().unwrap(),
                                    ))
                                });
                                drop(mapped);
                                if sample.into_iter().all(f32::is_finite) {
                                    Ok(sample)
                                } else {
                                    Err("Pawn fluid sample contains a non-finite value".to_owned())
                                }
                            })
                    });
                mapped_buffer.unmap();
                if let Ok(mut result) = result.lock() {
                    *result = Some(sample);
                }
            });
        Ok(())
    }

    /// Submits queued fluid exports and begins their asynchronous readbacks
    fn fluid_downloads_submit(&self) -> Result<(), io::Error> {
        for download in &self.fluid_downloads {
            let mut state = download
                .lock()
                .map_err(|_| io::Error::other("Fluid download is unavailable"))?;
            if state.is_started {
                continue;
            }
            self.fluids.export(
                self.accelerator.as_ref(),
                &state,
                self.area_fluid_active().origin(),
                self.area_fluid_active().dimensions()[0],
                self.area_fluid_active().dimensions()[1],
                self.area_buffered().origin(),
                self.area_buffered().dimensions()[0],
                self.area_buffered().dimensions()[1],
                self.tiles_ring_offset_x,
                self.tiles_ring_offset_y,
            );
            state.is_started = true;
            let buffer: wgpu::Buffer = state.buffer.clone();
            let mapped_buffer: wgpu::Buffer = buffer.clone();
            let download: Arc<Mutex<FluidDownload>> = download.clone();
            let particle_capacity: u32 = self.fluids.particle_capacity();
            drop(state);
            buffer
                .slice(..)
                .map_async(wgpu::MapMode::Read, move |result| {
                    let bytes: Result<Vec<u8>, io::Error> = match result {
                        Ok(()) => match mapped_buffer.slice(..).get_mapped_range() {
                            Ok(mapped_data) => {
                                let count: usize =
                                    u32::from_le_bytes(mapped_data[0..4].try_into().unwrap())
                                        as usize;
                                let byte_count: usize = if count <= particle_capacity as usize {
                                    16 + count * ChunkFluidParticle::GPU_SIZE
                                } else {
                                    16
                                };
                                let bytes: Vec<u8> = mapped_data[..byte_count].to_vec();
                                drop(mapped_data);
                                mapped_buffer.unmap();
                                Ok(bytes)
                            }
                            Err(error) => {
                                mapped_buffer.unmap();
                                Err(io::Error::other(error.to_string()))
                            }
                        },
                        Err(_) => Err(io::Error::other("Fluid download failed")),
                    };
                    std::thread::spawn(move || {
                        let result: Result<Vec<ChunkFluidParticle>, io::Error> =
                            bytes.and_then(|bytes| {
                                FluidDownload::deserialize(&bytes, particle_capacity)
                            });
                        if let Ok(mut state) = download.lock() {
                            state.result = Some(result);
                        }
                    });
                });
        }
        Ok(())
    }

    /// Submits queued dormant-fluid reconstruction and begins result readback
    fn fluid_uploads_submit(&self) -> Result<(), io::Error> {
        for upload in &self.fluid_uploads {
            let mut state = upload
                .lock()
                .map_err(|_| io::Error::other("Fluid upload is unavailable"))?;
            if state.is_started {
                continue;
            }
            self.fluids.import(
                self.accelerator.as_ref(),
                &state,
                self.area_fluid_active().origin(),
                self.area_fluid_active().dimensions()[0],
                self.area_fluid_active().dimensions()[1],
                self.area_buffered().origin(),
                self.area_buffered().dimensions()[0],
                self.area_buffered().dimensions()[1],
                self.tiles_ring_offset_x,
                self.tiles_ring_offset_y,
            )?;
            state.is_started = true;
            let buffer: wgpu::Buffer = state.buffer.clone();
            let mapped_buffer: wgpu::Buffer = buffer.clone();
            let upload: Arc<Mutex<FluidUpload>> = upload.clone();
            drop(state);
            buffer
                .slice(..)
                .map_async(wgpu::MapMode::Read, move |result| {
                    let bytes: Result<Vec<u8>, io::Error> = match result {
                        Ok(()) => match mapped_buffer.slice(..).get_mapped_range() {
                            Ok(mapped_data) => {
                                let bytes: Vec<u8> = mapped_data.to_vec();
                                drop(mapped_data);
                                mapped_buffer.unmap();
                                Ok(bytes)
                            }
                            Err(error) => {
                                mapped_buffer.unmap();
                                Err(io::Error::other(error.to_string()))
                            }
                        },
                        Err(_) => Err(io::Error::other("Fluid upload result readback failed")),
                    };
                    std::thread::spawn(move || {
                        if let Ok(mut state) = upload.lock() {
                            let result = bytes.and_then(|bytes| state.failed_particles(&bytes));
                            state.result = Some(result);
                        }
                    });
                });
        }
        Ok(())
    }

    /// Queues tile downloads from the `Accelerator`
    pub fn tiles_download(
        &self,
        area: TileArea,
    ) -> impl Future<Output = Result<HashMap<TileCoordinates, TileData>, io::Error>> + 'static {
        let downloads: Vec<Arc<Mutex<TileDownload>>> = area
            .iterate_tile_coordinates()
            .filter_map(|coordinates| {
                self.tile_at(coordinates).map(|tile| {
                    Arc::new(Mutex::new(TileDownload::new(
                        self.accelerator.as_ref(),
                        coordinates,
                        tile,
                    )))
                })
            })
            .collect();
        let mut error: Option<io::Error> = None;
        if let Err(_) = self.tile_downloads.lock().map(|mut tile_downloads| {
            tile_downloads.extend(downloads.iter().cloned());
        }) {
            error = Some(io::Error::other("Tile download queue is unavailable"));
        }
        let mut downloads: Vec<Arc<Mutex<TileDownload>>> = downloads;
        let mut tile_data: HashMap<TileCoordinates, TileData> = HashMap::new();
        poll_fn(move |context| {
            if let Some(error) = error.take() {
                return std::task::Poll::Ready(Err(error));
            }
            let mut index: usize = 0;
            while index < downloads.len() {
                let mut download: std::sync::MutexGuard<TileDownload> =
                    downloads[index].lock().unwrap();
                match download.result.take() {
                    Some(Ok(data)) => {
                        let coordinates: TileCoordinates = download.coordinates;
                        drop(download);
                        downloads.swap_remove(index);
                        tile_data.insert(coordinates, data);
                    }
                    Some(Err(error)) => return std::task::Poll::Ready(Err(error)),
                    None => {
                        download.waker = Some(context.waker().clone());
                        index += 1;
                    }
                }
            }
            std::task::Poll::Ready(Ok(std::mem::take(&mut tile_data)))
        })
    }

    /// Queues tile uploads to the `Accelerator`
    pub fn tiles_upload(
        &mut self,
        area: TileArea,
    ) -> impl Future<Output = Result<(), io::Error>> + 'static {
        let mut error: Option<io::Error> = None;
        let mut uploads: Vec<Arc<Mutex<TileUpload>>> = Vec::new();
        let materials = self.data.materials();
        let ambient_temperature = self.ambient_temperature;
        for coordinates in area.iterate_tile_coordinates() {
            if self.tile_at(coordinates).is_none() {
                continue;
            }
            match self.chunks.get_mut(&coordinates.chunk_coordinates()) {
                Some(ChunkEntry::Active { chunk, .. }) => match chunk.get_tile_mut(coordinates) {
                    Ok(tile_data) => {
                        let tile_data = tile_data;
                        tile_data.resolve_uninitialized_temperatures(|identifier| {
                            materials
                                .thermal_properties(identifier)
                                .and_then(|properties| properties.default_temperature)
                                .unwrap_or(ambient_temperature)
                        });
                        let mut upload = TileUpload::new(coordinates, tile_data);
                        upload.resolve_uninitialized_state(|identifier| {
                            self.initial_temperature(identifier)
                        });
                        uploads.push(Arc::new(Mutex::new(upload)));
                    }
                    Err(()) => {
                        error = Some(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "Tile is not in its active chunk",
                        ));
                        break;
                    }
                },
                _ => {
                    error = Some(io::Error::new(
                        io::ErrorKind::NotFound,
                        "Tile chunk is not active",
                    ));
                    break;
                }
            }
        }
        if error.is_none()
            && let Err(_) = self.tile_uploads.lock().map(|mut tile_uploads| {
                tile_uploads.extend(uploads.iter().cloned());
            })
        {
            error = Some(io::Error::other("Tile upload queue is unavailable"));
        }
        poll_fn(move |context| {
            if let Some(error) = error.take() {
                return std::task::Poll::Ready(Err(error));
            }
            let mut index: usize = 0;
            while index < uploads.len() {
                let mut upload: std::sync::MutexGuard<TileUpload> = uploads[index].lock().unwrap();
                match upload.result.take() {
                    Some(Ok(())) => {
                        drop(upload);
                        uploads.swap_remove(index);
                    }
                    Some(Err(error)) => return std::task::Poll::Ready(Err(error)),
                    None => {
                        upload.waker = Some(context.waker().clone());
                        index += 1;
                    }
                }
            }
            std::task::Poll::Ready(Ok(()))
        })
    }

    /// Queues mandatory downloads for tiles leaving Accelerator residency
    fn tile_downloads_queue(&mut self, area: TileArea) -> Result<(), io::Error> {
        // bind every world coordinate to its physical slot under the old ring mapping
        let mut downloads: Vec<Arc<Mutex<TileDownload>>> = Vec::new();
        for coordinates in area.iterate_tile_coordinates() {
            let tile: Tile = self.tile_at(coordinates).ok_or_else(|| {
                io::Error::other("Outgoing tile is outside the old Accelerator buffer")
            })?;
            downloads.push(Arc::new(Mutex::new(TileDownload::new(
                self.accelerator.as_ref(),
                coordinates,
                tile,
            ))));
        }
        // share the existing copy and deserialization path while retaining internal ownership
        self.tile_downloads
            .lock()
            .map_err(|_| io::Error::other("Tile download queue is unavailable"))?
            .extend(downloads.iter().cloned());
        self.outgoing_tile_downloads.extend(downloads);
        Ok(())
    }

    /// Returns whether an area contains an outgoing tile awaiting download
    fn tile_download_pending_in(&self, area: TileArea) -> Result<bool, io::Error> {
        for download in &self.outgoing_tile_downloads {
            let coordinates: TileCoordinates = download
                .lock()
                .map_err(|_| io::Error::other("Outgoing tile download is unavailable"))?
                .coordinates;
            if area.contains(coordinates) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Returns whether a chunk contains an outgoing tile awaiting download
    fn tile_download_pending_for_chunk(
        &self,
        coordinates: TileCoordinates,
    ) -> Result<bool, io::Error> {
        for download in &self.outgoing_tile_downloads {
            let tile_coordinates: TileCoordinates = download
                .lock()
                .map_err(|_| io::Error::other("Outgoing tile download is unavailable"))?
                .coordinates;
            if tile_coordinates.chunk_coordinates() == coordinates {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Applies completed outgoing tile downloads to persistent CPU chunks
    fn tile_downloads_apply_completed(&mut self) -> Result<(), io::Error> {
        let mut index: usize = 0;
        while index < self.outgoing_tile_downloads.len() {
            // leave unfinished and failed jobs pinned so stale chunks cannot be saved
            let mut download: std::sync::MutexGuard<TileDownload> = self.outgoing_tile_downloads
                [index]
                .lock()
                .map_err(|_| io::Error::other("Outgoing tile download is unavailable"))?;
            let Some(result) = download.result.as_ref() else {
                index += 1;
                continue;
            };
            if let Err(error) = result {
                return Err(io::Error::other(format!(
                    "Outgoing tile download failed: {error}"
                )));
            }
            let coordinates: TileCoordinates = download.coordinates;
            if !matches!(
                self.chunks.get(&coordinates.chunk_coordinates()),
                Some(ChunkEntry::Active { .. }),
            ) {
                return Err(io::Error::other(
                    "Outgoing tile download chunk is not active",
                ));
            }
            let tile_data: TileData = download.result.take().unwrap().unwrap();
            drop(download);

            // replace the stale persistence copy and route saving through normal dirty handling
            let Some(ChunkEntry::Active { chunk, is_dirty }) =
                self.chunks.get_mut(&coordinates.chunk_coordinates())
            else {
                unreachable!();
            };
            chunk.set_tile(coordinates, tile_data).map_err(|_| {
                io::Error::other("Outgoing tile download is outside its active chunk")
            })?;
            *is_dirty = true;
            self.outgoing_tile_downloads.swap_remove(index);
        }
        Ok(())
    }

    /// Submits queued Accelerator tile downloads
    fn tile_downloads_submit(&self) -> Result<(), io::Error> {
        // acquire the download queue
        let mut downloads_started: Vec<(Arc<Mutex<TileDownload>>, wgpu::Buffer)> = Vec::new();
        let mut command_encoder: Option<wgpu::CommandEncoder> = None;
        {
            let downloads = self
                .tile_downloads
                .lock()
                .map_err(|_| io::Error::other("Tile download queue is unavailable"))?;
            // process each incomplete download
            for download in downloads.iter() {
                let mut state: std::sync::MutexGuard<TileDownload> = download.lock().unwrap();
                if state.result.is_some() {
                    continue;
                }
                if state.is_started {
                    continue;
                }
                let tile: Tile = state.physical_tile;
                let command_encoder: &mut wgpu::CommandEncoder = command_encoder
                    .get_or_insert_with(|| {
                        self.accelerator.wgpu_device().create_command_encoder(
                            &wgpu::CommandEncoderDescriptor {
                                label: Some("tile_downloads_submit"),
                            },
                        )
                    });
                command_encoder.copy_buffer_to_buffer(
                    self.cellular_material_identifiers.wgpu_buffer(),
                    tile.0 as u64 * TileData::CELL_FIELD_SERIALIZED_SIZE as u64,
                    &state.buffer,
                    0,
                    TileData::CELL_FIELD_SERIALIZED_SIZE as u64,
                );
                command_encoder.copy_buffer_to_buffer(
                    self.cellular_appearances.wgpu_buffer(),
                    tile.0 as u64 * TileData::CELL_FIELD_SERIALIZED_SIZE as u64,
                    &state.buffer,
                    TileData::CELL_FIELD_SERIALIZED_SIZE as u64,
                    TileData::CELL_FIELD_SERIALIZED_SIZE as u64,
                );
                command_encoder.copy_buffer_to_buffer(
                    self.cellular_integrities.wgpu_buffer(),
                    tile.0 as u64 * TileData::CELL_FIELD_SERIALIZED_SIZE as u64,
                    &state.buffer,
                    TileData::CELL_FIELD_SERIALIZED_SIZE as u64 * 2,
                    TileData::CELL_FIELD_SERIALIZED_SIZE as u64,
                );
                command_encoder.copy_buffer_to_buffer(
                    self.cellular_amounts.wgpu_buffer(),
                    tile.0 as u64 * TileData::CELL_FIELD_SERIALIZED_SIZE as u64,
                    &state.buffer,
                    TileData::CELL_FIELD_SERIALIZED_SIZE as u64 * 3,
                    TileData::CELL_FIELD_SERIALIZED_SIZE as u64,
                );
                command_encoder.copy_buffer_to_buffer(
                    self.cellular_temperatures.wgpu_buffer(),
                    tile.0 as u64 * TileData::CELL_FIELD_SERIALIZED_SIZE as u64,
                    &state.buffer,
                    TileData::CELL_FIELD_SERIALIZED_SIZE as u64 * 4,
                    TileData::CELL_FIELD_SERIALIZED_SIZE as u64,
                );
                state.is_started = true;
                downloads_started.push((download.clone(), state.buffer.clone()));
            }
        }
        if let Some(command_encoder) = command_encoder {
            self.accelerator
                .wgpu_queue()
                .submit(Some(command_encoder.finish()));
            // submit the encoded copies and register their readback callbacks
            for (download, buffer) in downloads_started {
                let mapped_buffer: wgpu::Buffer = buffer.clone();
                buffer
                    .slice(..)
                    .map_async(wgpu::MapMode::Read, move |result| {
                        let result: Result<TileData, io::Error> = match result {
                            Ok(()) => match mapped_buffer.slice(..).get_mapped_range() {
                                Ok(mapped_data) => {
                                    let mut material_data: &[u8] =
                                        &mapped_data[..TileData::CELL_FIELD_SERIALIZED_SIZE];
                                    let mut appearance_data: &[u8] = &mapped_data
                                        [TileData::CELL_FIELD_SERIALIZED_SIZE
                                            ..TileData::CELL_FIELD_SERIALIZED_SIZE * 2];
                                    let mut integrity_data: &[u8] = &mapped_data
                                        [TileData::CELL_FIELD_SERIALIZED_SIZE * 2
                                            ..TileData::CELL_FIELD_SERIALIZED_SIZE * 3];
                                    let mut amount_data: &[u8] = &mapped_data
                                        [TileData::CELL_FIELD_SERIALIZED_SIZE * 3
                                            ..TileData::CELL_FIELD_SERIALIZED_SIZE * 4];
                                    let mut temperature_data: &[u8] =
                                        &mapped_data[TileData::CELL_FIELD_SERIALIZED_SIZE * 4..];
                                    let tile_data: Result<TileData, io::Error> =
                                        TileData::deserialize_fields(
                                            &mut material_data,
                                            &mut appearance_data,
                                            &mut integrity_data,
                                            &mut amount_data,
                                            &mut temperature_data,
                                        );
                                    drop(mapped_data);
                                    mapped_buffer.unmap();
                                    tile_data
                                }
                                Err(error) => {
                                    mapped_buffer.unmap();
                                    Err(io::Error::other(error.to_string()))
                                }
                            },
                            Err(_) => Err(io::Error::other("Tile download failed")),
                        };
                        let mut download: std::sync::MutexGuard<TileDownload> =
                            download.lock().unwrap();
                        download.result = Some(result);
                        download.is_complete = true;
                        if let Some(waker) = download.waker.take() {
                            waker.wake();
                        }
                    });
            }
        }
        Ok(())
    }

    /// Removes completed Accelerator tile downloads
    fn tile_downloads_clean(&self) -> Result<(), io::Error> {
        self.tile_downloads
            .lock()
            .map_err(|_| io::Error::other("Tile download queue is unavailable"))?
            .retain(|download| !download.lock().unwrap().is_complete);
        Ok(())
    }

    /// Submits queued Accelerator tile uploads
    fn tile_uploads_submit(&self) -> Result<(), io::Error> {
        // acquire the pending upload queue
        let uploads = self
            .tile_uploads
            .lock()
            .map_err(|_| io::Error::other("Tile upload queue is unavailable"))?;
        // process each incomplete upload
        for upload in uploads.iter() {
            let mut state: std::sync::MutexGuard<TileUpload> = upload.lock().unwrap();
            if state.result.is_some() {
                continue;
            }
            let Some(tile) = self.tile_at(state.coordinates) else {
                state.result = Some(Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Tile is outside the Accelerator buffer",
                )));
                state.is_complete = true;
                if let Some(waker) = state.waker.take() {
                    waker.wake();
                }
                continue;
            };
            self.accelerator.wgpu_queue().write_buffer(
                self.cellular_material_identifiers.wgpu_buffer(),
                tile.0 as u64 * TileData::CELL_FIELD_SERIALIZED_SIZE as u64,
                &state.material_identifiers,
            );
            self.accelerator.wgpu_queue().write_buffer(
                self.cellular_appearances.wgpu_buffer(),
                tile.0 as u64 * TileData::CELL_FIELD_SERIALIZED_SIZE as u64,
                &state.appearances,
            );
            self.accelerator.wgpu_queue().write_buffer(
                self.cellular_integrities.wgpu_buffer(),
                tile.0 as u64 * TileData::CELL_FIELD_SERIALIZED_SIZE as u64,
                &state.integrities,
            );
            self.accelerator.wgpu_queue().write_buffer(
                self.cellular_amounts.wgpu_buffer(),
                tile.0 as u64 * TileData::CELL_FIELD_SERIALIZED_SIZE as u64,
                &state.amounts,
            );
            self.accelerator.wgpu_queue().write_buffer(
                self.cellular_temperatures.wgpu_buffer(),
                tile.0 as u64 * TileData::CELL_FIELD_SERIALIZED_SIZE as u64,
                &state.temperatures,
            );
            self.cellular_dynamic.clear_cellular_dynamic_kinematics(
                self.accelerator.as_ref(),
                tile.0 as usize * 64,
                64,
            );
            self.cellular_pressure.clear_transient_state(
                self.accelerator.as_ref(),
                tile.0 as usize * 64,
                64,
            );
            state.result = Some(Ok(()));
            state.is_complete = true;
            if let Some(waker) = state.waker.take() {
                waker.wake();
            }
        }
        Ok(())
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

#[cfg(test)]
mod tests {

    use super::*;
    use crate::materials::{
        MaterialReaction, MaterialReactionReactant, MaterialReference, MaterialRegistryBuilder,
        MaterialThermalProperties, MaterialThermalTransition,
    };
    use crate::scenes::tests::scene_test_readback::{
        read_amount, read_cell_state, read_fluid_state,
    };
    use engine_graphics::{Color, MaterialAppearance};
    use std::{sync::mpsc, time::Instant};

    #[test]
    fn gas_leaves_and_returns_through_ring_streaming() {
        let _accelerator_test_lock = crate::simulation::tests::acquire_accelerator_test_lock();
        let accelerator: Arc<Accelerator> = Arc::new(Accelerator::new().unwrap());
        let mut materials: MaterialRegistry = MaterialRegistry::new();
        let vapor: MaterialIdentifier = materials.register(Material::Gas {
            name: "Vapor".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(120, 160, 190)),
            density: 0.65,
            diffusivity: 0.12,
            extinction: 0.08,
            dissipation: 0.0,
            compressibility: 0.05,
        });
        let mut scene: Scene = Scene::new(
            &accelerator,
            materials,
            SceneSimulationConfiguration {
                gravity: [0.0, -18.0],
                ambient_temperature: 293.15,
                empty_space_thermal_conductivity: 0.0,
                empty_space_heat_capacity: 1.0,
                maximum_gas_concentration: 4.0,
                width: 4,
                height: 4,
                buffer_size: 2,
                streaming_batch_size: 1,
            },
        )
        .unwrap();
        let coordinates: CellCoordinates = CellCoordinates { x: -16, y: 0 };
        let mut edits: SceneEditBatch = SceneEditBatch::new();
        edits.place_material(vapor, CellularAppearance::NEUTRAL, vec![coordinates]);
        scene.apply_edits_immediate(&mut edits).unwrap();
        scene.shift_to(TileCoordinates { x: 1, y: 0 }).unwrap();
        scene.origin_target = scene.origin;
        let started: Instant = Instant::now();
        while !scene.gas_downloads.is_empty()
            || !scene.fluid_downloads.is_empty()
            || !scene.outgoing_tile_downloads.is_empty()
        {
            scene.update(Duration::ZERO, false).unwrap();
            assert!(started.elapsed() < Duration::from_secs(5));
            std::thread::yield_now();
        }
        scene.shift_to(TileCoordinates { x: 0, y: 0 }).unwrap();

        let area: TileArea = TileArea::new(TileCoordinates { x: -2, y: 0 }, 1, 1);
        let download: GasDownload =
            GasDownload::new(accelerator.as_ref(), area, 64, scene.gases.gas_count());
        let buffered_area: TileArea = scene.area_buffered();
        let dimensions: [u16; 2] = buffered_area.dimensions();
        scene.gases.export(
            accelerator.as_ref(),
            &download,
            buffered_area.origin(),
            dimensions[0],
            dimensions[1],
            scene.tiles_ring_offset_x,
            scene.tiles_ring_offset_y,
        );
        let byte_count: u64 = 64 * u64::from(5 + scene.gases.gas_count()) * 4;
        let (sender, receiver) = mpsc::sync_channel(1);
        download
            .buffer
            .slice(0..byte_count)
            .map_async(wgpu::MapMode::Read, move |result| {
                sender.send(result).unwrap();
            });
        let started: Instant = Instant::now();
        loop {
            accelerator.poll().unwrap();
            if let Ok(result) = receiver.try_recv() {
                result.unwrap();
                break;
            }
            assert!(started.elapsed() < Duration::from_secs(5));
            std::thread::yield_now();
        }
        let mapped = download
            .buffer
            .slice(0..byte_count)
            .get_mapped_range()
            .unwrap();
        let bytes: Vec<u8> = mapped.to_vec();
        drop(mapped);
        download.buffer.unmap();
        let restored: Vec<ChunkGasCell> = GasDownload::deserialize(&bytes, area, &[vapor]).unwrap();
        assert!(
            restored
                .iter()
                .any(|cell| cell.coordinates == coordinates && cell.species == vec![(vapor, 1.0)])
        );
    }

    #[test]
    fn cellular_indirect_dispatch_executes() {
        let _accelerator_test_lock = crate::simulation::tests::acquire_accelerator_test_lock();
        let accelerator: Arc<Accelerator> = Arc::new(Accelerator::new().unwrap());
        let mut materials: MaterialRegistry = MaterialRegistry::new();
        let sand: MaterialIdentifier = materials.register(Material::CellularDynamic {
            name: "Sand".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(194, 178, 128)),
            mass: 1.0,
            pressure_transmission: 0.35,
            friction: 0.65,
            restitution: 0.05,
        });
        let mut scene: Scene = Scene::new(
            &accelerator,
            materials,
            SceneSimulationConfiguration {
                gravity: [0.0, -18.0],
                ambient_temperature: 293.15,
                empty_space_thermal_conductivity: 0.0,
                empty_space_heat_capacity: 1.0,
                maximum_gas_concentration: 4.0,
                width: 4,
                height: 4,
                buffer_size: 2,
                streaming_batch_size: 1,
            },
        )
        .unwrap();
        let mut edits: SceneEditBatch = SceneEditBatch::new();
        edits.place_material(
            sand,
            CellularAppearance::NEUTRAL,
            vec![CellCoordinates { x: 0, y: 8 }],
        );
        scene.apply_edits_immediate(&mut edits).unwrap();
        scene.update(Duration::from_secs(1) / 60, true).unwrap();
        accelerator.poll().unwrap();
    }

    #[test]
    fn acid_fluid_erodes_same_cell_and_cardinal_stone_across_ticks() {
        let _accelerator_test_lock = crate::simulation::tests::acquire_accelerator_test_lock();
        let accelerator = Arc::new(Accelerator::new().unwrap());
        let mut materials = MaterialRegistryBuilder::new();
        let stone = materials.register(Material::CellularDynamic {
            name: "Stone".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(100, 100, 100)),
            mass: 1.0,
            pressure_transmission: 1.0,
            friction: 0.5,
            restitution: 0.0,
        });
        let acid = materials.register(Material::Fluid {
            name: "Acid".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(80, 220, 70)),
            pressure_transmission: 1.0,
            friction: 0.0,
            restitution: 0.0,
            rest_density: 1.0,
            artificial_pressure: 0.0,
            xsph_smoothing: 0.0,
            body_push_speed: 0.0,
            density: 1.0,
            viscosity: 1.0,
        });
        materials.tag(stone, "corrodable").unwrap();
        materials.register_reaction(MaterialReaction {
            reactants: [
                Some(MaterialReactionReactant {
                    selector: MaterialReference::Material(acid),
                    amount: 0.2,
                }),
                Some(MaterialReactionReactant {
                    selector: MaterialReference::Tag("corrodable".into()),
                    amount: 1.0,
                }),
            ],
            maximum_extent_per_tick: 0.15,
            thermal_energy: 0.01,
            ..Default::default()
        });
        let compiled_materials = materials.compile().unwrap();
        assert_eq!(compiled_materials.reactions().len(), 1);
        assert_eq!(compiled_materials.reaction_selector_members()[1], stone);
        let mut scene = Scene::new(
            &accelerator,
            compiled_materials,
            SceneSimulationConfiguration {
                gravity: [0.0, 0.0],
                ambient_temperature: 293.15,
                empty_space_thermal_conductivity: 0.0,
                empty_space_heat_capacity: 1.0,
                maximum_gas_concentration: 4.0,
                width: 4,
                height: 4,
                buffer_size: 2,
                streaming_batch_size: 1,
            },
        )
        .unwrap();
        let acid_cell = CellCoordinates { x: 0, y: 8 };
        let cardinal_stone_cell = CellCoordinates { x: 1, y: 8 };
        let same_cell = CellCoordinates { x: 2, y: 8 };
        scene.update(Duration::ZERO, false).unwrap();
        let mut edits = SceneEditBatch::new();
        edits.place_material(acid, CellularAppearance::NEUTRAL, vec![acid_cell]);
        edits.place_material(
            stone,
            CellularAppearance::NEUTRAL,
            vec![cardinal_stone_cell, same_cell],
        );
        scene.queue_edits(edits);
        scene.update(Duration::ZERO, false).unwrap();
        scene.update(Duration::ZERO, false).unwrap();
        accelerator.poll().unwrap();
        let cardinal_index = scene.cell_edit_index(cardinal_stone_cell).unwrap();
        let same_index = scene.cell_edit_index(same_cell).unwrap();
        eprintln!(
            "initial: cardinal={:?} same={:?}",
            read_cell_state(accelerator.as_ref(), &scene, cardinal_index),
            read_cell_state(accelerator.as_ref(), &scene, same_index)
        );
        let mut previous = 1.0;
        let mut sequence = Vec::new();
        for tick in 1..=7 {
            scene.update(Duration::from_secs(1) / 60, true).unwrap();
            let (_, cardinal_amount) =
                read_cell_state(accelerator.as_ref(), &scene, cardinal_index);
            assert!(cardinal_amount <= previous + 0.00001);
            assert!((cardinal_amount - (1.0 - tick as f32 * 0.15).max(0.0)).abs() < 0.0001);
            sequence.push(cardinal_amount);
            previous = cardinal_amount;
        }
        eprintln!("multi-tick Acid erosion: {sequence:?}");

        scene.fluids.commit_reserved_particle(
            accelerator.as_ref(),
            scene.fluids.particle_capacity() - 1,
            acid.as_u32(),
            [same_cell.x as f32 + 0.5, same_cell.y as f32 + 0.5].map(|coordinate| coordinate / 8.0),
            [0.0, 0.0],
            0.8,
            293.15,
        );
        accelerator.wgpu_queue().write_buffer(
            scene
                .test_cellular_material_identifiers_buffer()
                .wgpu_buffer(),
            same_index as u64 * 4,
            &stone.as_u32().to_le_bytes(),
        );
        accelerator.wgpu_queue().write_buffer(
            scene.test_cellular_amounts_buffer().wgpu_buffer(),
            same_index as u64 * 4,
            &1.0f32.to_le_bytes(),
        );
        accelerator.poll().unwrap();
        let mut same_sequence = Vec::new();
        for tick in 1..=7 {
            scene.update(Duration::from_secs(1) / 60, true).unwrap();
            let (same_material, same_amount) =
                read_cell_state(accelerator.as_ref(), &scene, same_index);
            let (acid_material, acid_active, acid_amount) = read_fluid_state(
                accelerator.as_ref(),
                &scene,
                scene.fluids.particle_capacity() - 1,
            );
            assert_eq!(acid_material, acid.as_u32());
            assert_eq!(acid_active, 1);
            assert!(acid_amount > 0.000001);
            assert!((same_amount - (1.0 - tick as f32 * 0.15).max(0.0)).abs() < 0.0001);
            if tick < 7 {
                assert_eq!(same_material, stone.as_u32());
            } else {
                assert_eq!(same_material, MaterialIdentifier::NULL.as_u32());
            }
            same_sequence.push(same_amount);
        }
        eprintln!("same-cell Acid erosion: {same_sequence:?}");
    }

    #[test]
    fn acid_fluid_erodes_rigid_stone_and_removes_topology() {
        let _accelerator_test_lock = crate::simulation::tests::acquire_accelerator_test_lock();
        let accelerator = Arc::new(Accelerator::new().unwrap());
        let mut materials = MaterialRegistryBuilder::new();
        let stone = materials.register(Material::CellularStatic {
            name: "Stone".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(100, 100, 100)),
            mass: 1.0,
            pressure_ignore_threshold: 1.0,
            default_integrity: 1.0,
            minimum_rigid_body_cell_count: 1,
            debris_material: None,
            debris_yield_rate: 0.0,
            pressure_transmission: 1.0,
            friction: 0.5,
            restitution: 0.0,
        });
        let acid = materials.register(Material::Fluid {
            name: "Acid".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(80, 220, 70)),
            pressure_transmission: 1.0,
            friction: 0.0,
            restitution: 0.0,
            rest_density: 1.0,
            artificial_pressure: 0.0,
            xsph_smoothing: 0.0,
            body_push_speed: 0.0,
            density: 1.0,
            viscosity: 1.0,
        });
        materials.tag(stone, "corrodable").unwrap();
        materials.register_reaction(MaterialReaction {
            reactants: [
                Some(MaterialReactionReactant {
                    selector: MaterialReference::Material(acid),
                    amount: 0.2,
                }),
                Some(MaterialReactionReactant {
                    selector: MaterialReference::Tag("corrodable".into()),
                    amount: 1.0,
                }),
            ],
            maximum_extent_per_tick: 0.15,
            thermal_energy: 0.01,
            ..Default::default()
        });
        let mut scene = Scene::new(
            &accelerator,
            materials.compile().unwrap(),
            SceneSimulationConfiguration {
                gravity: [0.0, 0.0],
                ambient_temperature: 293.15,
                empty_space_thermal_conductivity: 0.0,
                empty_space_heat_capacity: 1.0,
                maximum_gas_concentration: 4.0,
                width: 4,
                height: 4,
                buffer_size: 2,
                streaming_batch_size: 1,
            },
        )
        .unwrap();
        scene.update(Duration::ZERO, false).unwrap();
        let acid_cell = CellCoordinates { x: 0, y: 8 };
        let stone_cell = CellCoordinates { x: 1, y: 8 };
        let mut edits = SceneEditBatch::new();
        edits.place_material(acid, CellularAppearance::NEUTRAL, vec![acid_cell]);
        edits.place_rigid_body(vec![SceneEditCellPlacement {
            coordinates: stone_cell,
            material_identifier: stone,
            appearance: CellularAppearance::NEUTRAL,
        }]);
        scene.queue_edits(edits);
        scene.update(Duration::ZERO, false).unwrap();
        scene.update(Duration::ZERO, false).unwrap();
        assert_eq!(scene.rigid_cellular_bodies.len(), 1);
        let state_slot = scene.rigid_cellular_bodies[0].cells[0].state_slot;
        let mut sequence = Vec::new();
        for _ in 1..=7 {
            scene.update(Duration::from_secs(1) / 60, true).unwrap();
            sequence.push(read_amount(
                accelerator.as_ref(),
                scene.rigid_cell_amounts_buffer(),
                state_slot,
            ));
        }
        assert!(sequence.windows(2).all(|pair| pair[1] <= pair[0] + 0.00001));
        for (tick, amount) in sequence.iter().enumerate() {
            assert!((*amount - (1.0 - (tick + 1) as f32 * 0.15).max(0.0)).abs() < 0.0001);
        }
        for _ in 0..3 {
            scene.update(Duration::ZERO, false).unwrap();
        }
        assert!(scene.rigid_cellular_bodies.is_empty());
        eprintln!("rigid Acid erosion: {sequence:?}");
    }

    #[test]
    fn full_screen_moving_sand_headless_tps() {
        let _accelerator_test_lock = crate::simulation::tests::acquire_accelerator_test_lock();
        let accelerator: Arc<Accelerator> = Arc::new(Accelerator::new().unwrap());
        let mut materials = MaterialRegistry::new();
        let sand = materials.register(Material::CellularDynamic {
            name: "Sand".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(194, 178, 128)),
            mass: 1.0,
            pressure_transmission: 0.35,
            friction: 0.65,
            restitution: 0.05,
        });
        let data_path = std::env::temp_dir().join(format!(
            "dogwood-sand-stress-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&data_path).unwrap();
        materials
            .serialize(&mut std::fs::File::create(data_path.join("materials")).unwrap())
            .unwrap();
        let data = SceneData::load(data_path.clone()).unwrap();
        let mut scene = Scene::load(
            &accelerator,
            SceneSimulationConfiguration {
                gravity: [0.0, -18.0],
                ambient_temperature: 293.15,
                empty_space_thermal_conductivity: 0.0,
                empty_space_heat_capacity: 1.0,
                maximum_gas_concentration: 4.0,
                width: 8,
                height: 8,
                buffer_size: 2,
                streaming_batch_size: 1,
            },
            data,
        )
        .unwrap();
        let mut edits = SceneEditBatch::new();
        edits.place_material(
            sand,
            CellularAppearance::NEUTRAL,
            (8..64)
                .step_by(2)
                .flat_map(|y| (0..64).map(move |x| CellCoordinates { x, y }))
                .collect(),
        );
        scene.apply_edits_immediate(&mut edits).unwrap();
        let tick = Duration::from_secs(1) / 60;
        for _ in 0..5 {
            scene.update(tick, true).unwrap();
        }
        let start = Instant::now();
        let mut older_snapshots = 0;
        let mut maximum_snapshot_age = 0;
        for _ in 0..30 {
            scene.update(tick, true).unwrap();
            let age = scene
                .physics_world
                .terrain_bridge_statistics()
                .collision_snapshot_age;
            maximum_snapshot_age = maximum_snapshot_age.max(age);
            older_snapshots += u32::from(age > 1);
        }
        let elapsed = start.elapsed();
        let stats = scene.physics_world.terrain_bridge_statistics();
        assert_eq!(stats.dynamic_shape_rebuilds, 0);
        assert_eq!(stats.dynamic_cells_scanned, 0);
        eprintln!(
            "moving sand headless TPS: {:.1}, snapshot age max {}, ticks >1 {}",
            30.0 / elapsed.as_secs_f64(),
            maximum_snapshot_age,
            older_snapshots
        );
        drop(scene);
        std::fs::remove_dir_all(data_path).unwrap();
    }

    #[test]
    fn disconnected_static_component_becomes_one_falling_rigid_body() {
        let _accelerator_test_lock = crate::simulation::tests::acquire_accelerator_test_lock();
        let accelerator: Arc<Accelerator> = Arc::new(Accelerator::new().unwrap());
        let mut materials: MaterialRegistry = MaterialRegistry::new();
        let stone: MaterialIdentifier = materials.register(Material::CellularStatic {
            name: "Stone".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(110, 105, 100)),
            mass: 1.0,
            pressure_ignore_threshold: 1.0,
            default_integrity: 1.0,
            minimum_rigid_body_cell_count: 4,
            debris_material: None,
            debris_yield_rate: 0.0,
            pressure_transmission: 0.5,
            friction: 0.7,
            restitution: 0.05,
        });
        let mut scene: Scene = Scene::new(
            &accelerator,
            materials,
            SceneSimulationConfiguration {
                gravity: [0.0, -8.0],
                ambient_temperature: 293.15,
                empty_space_thermal_conductivity: 0.0,
                empty_space_heat_capacity: 1.0,
                maximum_gas_concentration: 4.0,
                width: 1,
                height: 1,
                buffer_size: 2,
                streaming_batch_size: 1,
            },
        )
        .unwrap();
        let cells: Vec<CellCoordinates> = (2..=5)
            .flat_map(|x| (3..=4).map(move |y| CellCoordinates { x, y }))
            .chain((0..=2).map(|y| CellCoordinates { x: 3, y }))
            .collect();
        let mut edits = SceneEditBatch::new();
        edits.place_material(stone, CellularAppearance::NEUTRAL, cells.clone());
        scene.apply_edits_immediate(&mut edits).unwrap();
        let mut static_masks = vec![[0u32; 2]; 25];
        for cell in &cells {
            let tile_x = cell.x.div_euclid(8) + 2;
            let tile_y = cell.y.div_euclid(8) + 2;
            let tile = (tile_y * 5 + tile_x) as usize;
            let local = (cell.y.rem_euclid(8) * 8 + cell.x.rem_euclid(8)) as usize;
            static_masks[tile][local / 32] |= 1 << (local % 32);
        }
        let baseline = CollisionOccupancySnapshot {
            sequence: 0,
            origin: TileCoordinates { x: -2, y: -2 },
            width: 5,
            height: 5,
            static_masks: static_masks.into_boxed_slice(),
            dynamic_masks: vec![[0, 0]; 25].into_boxed_slice(),
        };
        scene
            .detach_unanchored_static_components(&mut baseline.clone())
            .unwrap();
        assert!(scene.rigid_cellular_bodies.is_empty());
        let mut separated = baseline.clone();
        separated.sequence = 1;
        separated.clear_static_cell(3, 2);
        scene
            .detach_unanchored_static_components(&mut separated)
            .unwrap();
        for _ in 0..20 {
            accelerator.poll().unwrap();
            scene.apply_completed_static_detachment().unwrap();
            if scene.rigid_cellular_bodies.len() == 1 {
                break;
            }
            std::thread::yield_now();
        }
        assert!(scene.rigid_cellular_bodies.len() == 1);
        assert!(scene.rigid_cellular_bodies[0].cells.len() == 8);
        let initial_y = scene
            .physics_world
            .rigid_cellular_body_state(&scene.rigid_cellular_bodies[0])
            .unwrap()
            .translation[1];
        scene.physics_world.update_cellular_snapshot(separated);
        for _ in 0..8 {
            scene
                .physics_world
                .step(scene.gravity, 1.0 / TICK_RATE as f32);
        }
        let state = scene
            .physics_world
            .rigid_cellular_body_state(&scene.rigid_cellular_bodies[0])
            .unwrap();
        assert!(state.translation[1] < initial_y);
        scene.cellular_physics_body_proxy.rasterize(
            accelerator.as_ref(),
            TileCoordinates { x: -2, y: -2 },
            5,
            5,
            0,
            0,
            scene.gravity,
            &[],
            &scene.rigid_cellular_bodies,
            &[state],
            scene.rigid_cellular_topology_revision,
        );
        accelerator.poll().unwrap();
    }

    #[test]
    fn accelerator_phase_static_cells_resolve_to_debris_or_rigid_body() {
        let _accelerator_test_lock = crate::simulation::tests::acquire_accelerator_test_lock();
        let accelerator = Arc::new(Accelerator::new().unwrap());
        let mut builder = MaterialRegistryBuilder::new();
        let debris = builder.register(Material::CellularDynamic {
            name: "Debris".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(150, 120, 90)),
            mass: 1.0,
            pressure_transmission: 1.0,
            friction: 0.5,
            restitution: 0.0,
        });
        let stone = builder.register(Material::CellularStatic {
            name: "Frozen Stone".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(110, 105, 100)),
            mass: 1.0,
            pressure_ignore_threshold: 1.0,
            default_integrity: 1.0,
            minimum_rigid_body_cell_count: 3,
            debris_material: Some(debris),
            debris_yield_rate: 1.0,
            pressure_transmission: 0.5,
            friction: 0.7,
            restitution: 0.05,
        });
        let fluid = builder.register(Material::Fluid {
            name: "Freezing Fluid".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(90, 180, 230)),
            pressure_transmission: 1.0,
            friction: 0.0,
            restitution: 0.0,
            rest_density: 1.0,
            artificial_pressure: 0.0,
            xsph_smoothing: 0.0,
            body_push_speed: 0.0,
            density: 1.0,
            viscosity: 1.0,
        });
        builder
            .set_thermal(
                debris,
                MaterialThermalProperties {
                    conductivity: 0.1,
                    specific_heat_capacity: 1.0,
                    default_temperature: Some(293.15),
                    ..Default::default()
                },
            )
            .unwrap();
        builder
            .set_thermal(
                stone,
                MaterialThermalProperties {
                    conductivity: 0.1,
                    specific_heat_capacity: 1.0,
                    default_temperature: Some(293.15),
                    ..Default::default()
                },
            )
            .unwrap();
        builder
            .set_thermal(
                fluid,
                MaterialThermalProperties {
                    conductivity: 0.1,
                    specific_heat_capacity: 1.0,
                    default_temperature: Some(400.0),
                    hot_transition: Some(MaterialThermalTransition {
                        threshold_temperature: 300.0,
                        target: stone,
                        yield_rate: 1.0,
                        latent_energy: 0.0,
                    }),
                    ..Default::default()
                },
            )
            .unwrap();
        let mut scene = Scene::new(
            &accelerator,
            builder.compile().unwrap(),
            SceneSimulationConfiguration {
                gravity: [0.0, 0.0],
                ambient_temperature: 293.15,
                empty_space_thermal_conductivity: 0.0,
                empty_space_heat_capacity: 1.0,
                maximum_gas_concentration: 4.0,
                width: 4,
                height: 4,
                buffer_size: 2,
                streaming_batch_size: 1,
            },
        )
        .unwrap();
        scene.update(Duration::from_secs(1) / 60, true).unwrap();
        let isolated = CellCoordinates { x: 8, y: 8 };
        let rigid_cells = [
            CellCoordinates { x: 16, y: 8 },
            CellCoordinates { x: 17, y: 8 },
            CellCoordinates { x: 16, y: 9 },
        ];
        let mut edits = SceneEditBatch::new();
        edits.place_material(
            fluid,
            CellularAppearance::NEUTRAL,
            std::iter::once(isolated)
                .chain(rigid_cells.iter().copied())
                .collect(),
        );
        scene.queue_edits(edits);
        scene.update(Duration::ZERO, false).unwrap();
        scene.update(Duration::ZERO, false).unwrap();
        for _ in 0..40 {
            scene.update(Duration::from_secs(1) / 60, true).unwrap();
            scene.update(Duration::ZERO, false).unwrap();
            if scene.rigid_cellular_bodies.len() == 1
                && read_cell_state(
                    accelerator.as_ref(),
                    &scene,
                    scene.cell_edit_index(isolated).unwrap(),
                )
                .0 == debris.as_u32()
            {
                break;
            }
        }
        let isolated_state = read_cell_state(
            accelerator.as_ref(),
            &scene,
            scene.cell_edit_index(isolated).unwrap(),
        );
        assert_eq!(isolated_state.0, debris.as_u32());
        assert_eq!(scene.rigid_cellular_bodies.len(), 1);
        assert_eq!(scene.rigid_cellular_bodies[0].cells.len(), 3);
        assert!(scene.rigid_cellular_bodies[0].cells.iter().all(
            |cell| cell.material == stone && cell.appearance.0 == CellularAppearance::NEUTRAL.0
        ));
    }

    #[test]
    fn queued_authored_rigid_body_is_atomic_and_body_local() {
        let _accelerator_test_lock = crate::simulation::tests::acquire_accelerator_test_lock();
        let accelerator = Arc::new(Accelerator::new().unwrap());
        let mut materials = MaterialRegistry::new();
        let stone = materials.register(Material::CellularStatic {
            name: "Stone".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(1, 1, 1)),
            mass: 2.0,
            pressure_ignore_threshold: 1.0,
            default_integrity: 1.0,
            minimum_rigid_body_cell_count: 1,
            debris_material: None,
            debris_yield_rate: 0.0,
            pressure_transmission: 1.0,
            friction: 0.7,
            restitution: 0.05,
        });
        let sand = materials.register(Material::CellularDynamic {
            name: "Sand".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(1, 1, 1)),
            mass: 1.0,
            pressure_transmission: 1.0,
            friction: 0.5,
            restitution: 0.0,
        });
        let mut scene = Scene::new(
            &accelerator,
            materials,
            SceneSimulationConfiguration {
                gravity: [0.0, -8.0],
                ambient_temperature: 293.15,
                empty_space_thermal_conductivity: 0.0,
                empty_space_heat_capacity: 1.0,
                maximum_gas_concentration: 4.0,
                width: 4,
                height: 4,
                buffer_size: 2,
                streaming_batch_size: 1,
            },
        )
        .unwrap();
        let mut edits = SceneEditBatch::new();
        edits.place_rigid_body(vec![
            SceneEditCellPlacement {
                coordinates: CellCoordinates { x: 10, y: 20 },
                material_identifier: stone,
                appearance: CellularAppearance(3),
            },
            SceneEditCellPlacement {
                coordinates: CellCoordinates { x: 11, y: 20 },
                material_identifier: stone,
                appearance: CellularAppearance(4),
            },
            SceneEditCellPlacement {
                coordinates: CellCoordinates { x: 10, y: 20 },
                material_identifier: stone,
                appearance: CellularAppearance(5),
            },
        ]);
        edits.place_rigid_body(vec![SceneEditCellPlacement {
            coordinates: CellCoordinates { x: 12, y: 20 },
            material_identifier: sand,
            appearance: CellularAppearance::NEUTRAL,
        }]);
        scene.queue_edits(edits);
        scene.update(Duration::ZERO, false).unwrap();
        assert_eq!(scene.rigid_cellular_bodies.len(), 1);
        let body = &scene.rigid_cellular_bodies[0];
        assert_eq!(body.cells.len(), 2);
        assert_eq!(body.cells[0].local, [0, 0]);
        assert_eq!(body.cells[1].local, [1, 0]);
        assert_eq!(body.cells[0].appearance.0, 5);
        assert_eq!(scene.rigid_cellular_topology_revision, 1);
    }
}
