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
#[path = "scene_edit_application.rs"]
mod scene_edit_application;
#[path = "scene_fluid_streaming.rs"]
mod scene_fluid_streaming;
#[path = "scene_gas_streaming.rs"]
mod scene_gas_streaming;
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

    /// Applies every compatible completed Accelerator reaction in submission order

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
