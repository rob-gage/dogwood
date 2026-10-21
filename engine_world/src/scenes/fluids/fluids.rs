// Copyright Rob Gage 2026

use super::{
    FluidActorContact,
    FluidParticleImport,
    fluid_particle_pass::{
        FluidEdit,
        FluidParticlePass,
    },
};
use crate::{
    actors::{
        Actor,
        components::ActorCollisionComponent,
    },
    materials::{
        MaterialForm,
        MaterialIdentifier,
        MaterialTable,
    },
    scenes::{
        cellular_tiles::{
            CellCoordinates,
            CellularTileCoordinates,
            CellularTileBuffers,
            CellularTileRegion,
            CELLULAR_TILE_SIZE,
        },
        chunks::{
            ChunkCoordinates,
            ChunkFluidParticle,
            CHUNK_SIZE,
        },
        collisions::CollisionRigidBodyPass,
        edits::{
            Edit,
            EditMode,
        },
        SceneError,
    },
};
use engine_host::acceleration::Accelerator;
use std::collections::{
    BTreeMap,
    BTreeSet,
    HashMap,
};

/// Bytes in one world-cell particle record written by the compaction export
const EXPORT_RECORD_BYTE_COUNT: usize = 40;

/// Fluid particle simulation of one scene's active cellular tile ring
pub struct Fluids {
    /// GPU particle storage and PBF simulation pass
    particle_pass: FluidParticlePass,
    /// Maximum live particles allocated with the cellular tile buffers
    capacity: u32,
    /// Latest queued action for each world cell that has been edited
    queued_edits: BTreeMap<CellCoordinates, FluidEdit>,
    /// Chunk particles admitted for exact-position imports
    queued_imports: Vec<FluidParticleImport>,
    /// Imported particles rejected by the GPU and awaiting return to their chunks
    rejected_imports: Vec<FluidParticleImport>,
    /// World cells whose current particles must be erased before queued spawns
    queued_erasures: BTreeSet<CellCoordinates>,
    /// Monotonic count of requested spawn operations, including overwritten attempts
    spawn_revision: u64,
    /// Count in the newest completed asynchronous GPU readback
    particle_count: Option<u32>,
    /// Spawn revision represented by the newest count readback
    particle_count_revision: u64,
    /// Submission number represented by the newest count readback
    particle_count_tick: u64,
    /// Region origin used by the previous simulation, in world cells
    previous_region_origin: Option<(i64, i64)>,
    /// Number of simulation submissions used to order asynchronous count readbacks
    tick: u64,
    /// Latest actor contacts whose collision settings still match the current actor records
    actor_contacts: BTreeMap<Actor, (ActorCollisionComponent, FluidActorContact, u64)>,
}

impl Fluids {

    /// Creates the fixed-capacity particle pool alongside its cellular tile buffers
    pub fn new(
        accelerator: &Accelerator,
        cellular_tile_buffers: &CellularTileBuffers,
        material_table: &MaterialTable,
        capacity_factor: f32,
        region_width: u32,
        region_height: u32,
    ) -> Self {
        let cell_count: u64 = cellular_tile_buffers.material_identifier_buffer().size() / 4;
        let capacity: u32 = (capacity_factor * cell_count as f32).floor() as u32;
        Self {
            particle_pass: FluidParticlePass::new(
                accelerator,
                cellular_tile_buffers,
                material_table,
                capacity,
                region_width,
                region_height,
            ),
            capacity,
            queued_edits: BTreeMap::new(),
            queued_imports: Vec::new(),
            rejected_imports: Vec::new(),
            queued_erasures: BTreeSet::new(),
            spawn_revision: 0,
            particle_count: None,
            particle_count_revision: 0,
            particle_count_tick: 0,
            previous_region_origin: None,
            tick: 0,
            actor_contacts: BTreeMap::new(),
        }
    }

    /// Returns whether a request's possible fluid spawns fit the reserved particle capacity
    pub fn can_queue_edit(&self, edit: &Edit) -> bool {
        let mut spawn_count: u64 = 0;
        for (cells, material_identifiers, _) in edit.cellular_tiles.values() {
            for (cell_index, material_identifier) in material_identifiers.iter().enumerate() {
                if cells & (1u64 << cell_index) != 0
                    && *material_identifier != MaterialIdentifier::EMPTY
                    && material_identifier.form() == MaterialForm::Fluid
                {
                    spawn_count += 1;
                }
            }
        }
        self.reserved_particle_count().saturating_add(spawn_count) <= u64::from(self.capacity)
    }

    /// Queues the particle effects of one already validated world-cell edit
    pub fn queue_edit(&mut self, edit: &Edit) {
        for (coordinates, (cells, material_identifiers, _)) in &edit.cellular_tiles {
            for cell_index in 0..64 {
                if cells & (1u64 << cell_index) == 0 { continue; }
                let cell: CellCoordinates = CellCoordinates {
                    x: coordinates.x * CELLULAR_TILE_SIZE + cell_index as i64 % CELLULAR_TILE_SIZE,
                    y: coordinates.y * CELLULAR_TILE_SIZE + cell_index as i64 / CELLULAR_TILE_SIZE,
                };
                let material_identifier: MaterialIdentifier = material_identifiers[cell_index];
                if material_identifier != MaterialIdentifier::EMPTY
                    && material_identifier.form() == MaterialForm::Fluid
                {
                    self.spawn_revision = self.spawn_revision.saturating_add(1);
                    let mode: u32 = if edit.mode == EditMode::Insert {
                        1
                    } else { 0 };
                    self.queued_edits.insert(cell, FluidEdit {
                        material_identifier,
                        mode,
                    });
                    if mode == 0 { self.queued_erasures.insert(cell); }
                } else {
                    self.queued_edits.remove(&cell);
                    self.queued_erasures.insert(cell);
                }
            }
        }
    }

    /// Returns the conservative number of imports that fit the reserved live capacity
    pub(crate) fn available_import_capacity(&self) -> usize {
        u64::from(self.capacity).saturating_sub(self.reserved_particle_count()) as usize
    }

    /// Queues selected chunk records at their exact active-region positions
    pub(crate) fn queue_chunk_particles(
        &mut self,
        active_region: CellularTileRegion,
        particles: Vec<(ChunkCoordinates, ChunkFluidParticle)>,
    ) -> Result<(), SceneError> {
        let origin_x: i64 = active_region.bottom_left.x * CELLULAR_TILE_SIZE;
        let origin_y: i64 = active_region.bottom_left.y * CELLULAR_TILE_SIZE;
        for (chunk_coordinates, particle) in particles {
            let position: [f32; 2] = particle.position();
            let whole_x: i64 = position[0].floor() as i64;
            let whole_y: i64 = position[1].floor() as i64;
            let chunk_origin_x: i64 = i64::from(chunk_coordinates.x) * CHUNK_SIZE;
            let chunk_origin_y: i64 = i64::from(chunk_coordinates.y) * CHUNK_SIZE;
            let local_x: i128 = i128::from(chunk_origin_x + whole_x) - i128::from(origin_x);
            let local_y: i128 = i128::from(chunk_origin_y + whole_y) - i128::from(origin_y);
            let region_width: i128 = active_region.width() as i128
                * i128::from(CELLULAR_TILE_SIZE);
            let region_height: i128 = active_region.height() as i128
                * i128::from(CELLULAR_TILE_SIZE);
            if local_x < 0 || local_y < 0 || local_x >= region_width || local_y >= region_height {
                return Err(SceneError::FluidParticleExportRecordInvalid);
            }
            let spawn_position: [f32; 2] = [
                local_x as f32 + (position[0] - whole_x as f32),
                local_y as f32 + (position[1] - whole_y as f32),
            ];
            self.queued_imports.push(FluidParticleImport {
                chunk_coordinates,
                particle,
                position: spawn_position,
            });
        }
        Ok(())
    }

    /// Polls rejected-import statuses and returns rejected chunk records to the caller
    pub(crate) fn poll_rejected_imports(
        &mut self,
        accelerator: &Accelerator,
    ) -> Result<Vec<(ChunkCoordinates, ChunkFluidParticle)>, SceneError> {
        let rejected: Vec<FluidParticleImport> =
            self.particle_pass.read_particle_import_readbacks(accelerator)?;
        self.rejected_imports.extend(rejected);
        Ok(self.rejected_imports.drain(..).map(|import| {
            (import.chunk_coordinates, import.particle)
        }).collect())
    }

    /// Restores rejected records after their chunk response could not be applied
    pub(crate) fn restore_rejected_imports(
        &mut self,
        particles: Vec<(ChunkCoordinates, ChunkFluidParticle)>,
    ) {
        self.rejected_imports.extend(particles.into_iter().map(|(
            chunk_coordinates,
            particle,
        )| FluidParticleImport {
            chunk_coordinates,
            particle,
            position: [0.0, 0.0],
        }));
    }

    /// Returns queued and GPU-rejected records before the scene's final resident snapshot
    pub(crate) fn drain_unspawned_particles_blocking(
        &mut self,
        accelerator: &Accelerator,
    ) -> Result<Vec<(ChunkCoordinates, ChunkFluidParticle)>, SceneError> {
        let mut imports: Vec<FluidParticleImport> =
            self.particle_pass.wait_particle_import_readbacks(accelerator)?;
        imports.append(&mut self.rejected_imports);
        imports.append(&mut self.queued_imports);
        Ok(imports.into_iter().map(|import| {
            (import.chunk_coordinates, import.particle)
        }).collect())
    }

    /// Returns whether imports still need capacity, status mapping, or chunk return
    pub(crate) fn has_pending_particle_imports(&self) -> bool {
        !self.queued_imports.is_empty()
            || !self.rejected_imports.is_empty()
            || self.particle_pass.has_pending_particle_imports()
    }

    /// Returns the tile region that owns the particle pool's current local coordinates
    pub(crate) fn particle_snapshot_region(
        &self,
        active_region: CellularTileRegion,
    ) -> CellularTileRegion {
        let origin: (i64, i64) = self.previous_region_origin.unwrap_or((
            active_region.bottom_left.x * CELLULAR_TILE_SIZE,
            active_region.bottom_left.y * CELLULAR_TILE_SIZE,
        ));
        let bottom_left: CellularTileCoordinates = CellularTileCoordinates {
            x: origin.0.div_euclid(CELLULAR_TILE_SIZE),
            y: origin.1.div_euclid(CELLULAR_TILE_SIZE),
        };
        CellularTileRegion {
            bottom_left,
            top_right: CellularTileCoordinates {
                x: bottom_left.x + active_region.width() as i64 - 1,
                y: bottom_left.y + active_region.height() as i64 - 1,
            },
        }
    }

    /// Starts an asynchronous copy of the resident live-particle pool
    pub(crate) fn request_particle_snapshot(
        &mut self,
        accelerator: &Accelerator,
        region: CellularTileRegion,
    ) -> bool {
        let origin: (i64, i64) = (
            region.bottom_left.x * CELLULAR_TILE_SIZE,
            region.bottom_left.y * CELLULAR_TILE_SIZE,
        );
        self.particle_pass.request_particle_snapshot(accelerator, origin)
    }

    /// Returns the completed live-particle snapshot, if its mapping has finished
    pub(crate) fn read_particle_snapshot(
        &mut self,
        accelerator: &Accelerator,
    ) -> Result<Option<Vec<(ChunkCoordinates, ChunkFluidParticle)>>, SceneError> {
        self.particle_pass.read_particle_snapshot(accelerator)
    }

    /// Returns whether a save snapshot is awaiting GPU mapping or chunk application
    pub(crate) fn has_pending_particle_snapshot(&self) -> bool {
        self.particle_pass.has_pending_particle_snapshot()
    }

    /// Copies all live particles for drop without unloading them first
    pub(crate) fn read_live_particles_blocking(
        &self,
        accelerator: &Accelerator,
        active_region: CellularTileRegion,
    ) -> Result<Vec<(ChunkCoordinates, ChunkFluidParticle)>, SceneError> {
        let origin: (i64, i64) = self.previous_region_origin.unwrap_or((
            active_region.bottom_left.x * CELLULAR_TILE_SIZE,
            active_region.bottom_left.y * CELLULAR_TILE_SIZE,
        ));
        self.particle_pass.read_live_particles_blocking(accelerator, origin)
    }

    /// Simulates one fixed tick using the actor records uploaded by kinematics and queues a
    /// live-particle count readback
    pub(crate) fn simulate(
        &mut self,
        accelerator: &Accelerator,
        active_region: CellularTileRegion,
        ring_offset: (u32, u32),
        gravity: f32,
        coupling_origin: (i32, i32),
        cellular_tile_buffers: &CellularTileBuffers,
        collision_rigid_body_pass: &CollisionRigidBodyPass,
        kinematics_actor_buffer: Option<&wgpu::Buffer>,
        actors: &[(Actor, ActorCollisionComponent)],
        export_target: Option<&wgpu::Buffer>,
        fluid_properties_buffer: &wgpu::Buffer,
    ) -> Result<(), SceneError> {
        let origin: (i64, i64) = (
            active_region.bottom_left.x * CELLULAR_TILE_SIZE,
            active_region.bottom_left.y * CELLULAR_TILE_SIZE,
        );
        let previous_origin: (i64, i64) = self.previous_region_origin.unwrap_or(origin);
        let rebase: [f32; 2] = [
            (previous_origin.0 as i128 - origin.0 as i128) as f32,
            (previous_origin.1 as i128 - origin.1 as i128) as f32,
        ];
        let edits: Vec<(CellCoordinates, FluidEdit)> = self.queued_edits.iter()
            .map(|(cell, edit)| (*cell, *edit))
            .collect();
        let erasures: Vec<CellCoordinates> = self.queued_erasures.iter().copied().collect();
        let import_count: usize = if self.particle_pass.can_submit_particle_imports() {
            self.queued_imports.len()
        } else { 0 };
        let imports: Vec<FluidParticleImport> = self.queued_imports.iter()
            .take(import_count)
            .copied()
            .collect();
        self.spawn_revision = self.spawn_revision.saturating_add(import_count as u64);
        let result: (
            Option<(u32, u64, u64)>,
            Vec<FluidParticleImport>,
            usize,
        ) = self.particle_pass.simulate(
            accelerator,
            active_region,
            ring_offset,
            origin,
            coupling_origin,
            rebase,
            gravity,
            cellular_tile_buffers,
            collision_rigid_body_pass,
            kinematics_actor_buffer,
            fluid_properties_buffer,
            actors,
            &edits,
            &erasures,
            &imports,
            export_target,
            self.tick,
            self.spawn_revision,
        )?;
        let (count_result, rejected_imports, submitted_import_count) = result;
        if submitted_import_count > 0 {
            self.queued_imports.drain(..submitted_import_count);
        }
        self.rejected_imports.extend(rejected_imports);
        if let Some((particle_count, tick, revision)) = count_result
            && tick >= self.particle_count_tick
        {
            self.particle_count = Some(particle_count);
            self.particle_count_tick = tick;
            self.particle_count_revision = revision;
        }
        self.queued_edits.clear();
        self.queued_erasures.clear();
        self.previous_region_origin = Some(origin);
        self.tick = self.tick.wrapping_add(1);
        Ok(())
    }

    /// Polls actor feedback without waiting and drops samples for replaced or absent actors
    pub(crate) fn poll_actor_contacts(
        &mut self,
        accelerator: &Accelerator,
        actors: &[(Actor, ActorCollisionComponent)],
    ) -> Result<HashMap<Actor, FluidActorContact>, SceneError> {
        let actors: BTreeMap<Actor, &ActorCollisionComponent> = actors.iter()
            .map(|(actor, collision)| (*actor, collision))
            .collect();
        self.actor_contacts.retain(|actor, (collision, _, _)| {
            actors.get(actor).is_some_and(|current| *current == collision)
        });
        for (tick, actor, collision, contact) in
            self.particle_pass.read_actor_contacts(accelerator)?
        {
            if !actors.get(&actor).is_some_and(|current| *current == &collision) { continue; }
            let replace: bool = self.actor_contacts.get(&actor)
                .is_none_or(|(_, _, previous_tick)| tick >= *previous_tick);
            if replace { self.actor_contacts.insert(actor, (collision, contact, tick)); }
        }
        Ok(self.actor_contacts.iter()
            .map(|(actor, (_, contact, _))| (*actor, *contact))
            .collect())
    }

    /// Returns the GPU raster of fluid mass, momentum, and material identifiers
    pub fn fluid_raster_buffer(&self) -> &wgpu::Buffer {
        self.particle_pass.fluid_raster_buffer()
    }

    /// Returns the fixed byte size of one compaction export readback
    pub(crate) fn particle_export_buffer_size(&self) -> u64 {
        self.particle_pass.particle_export_buffer_size()
    }

    /// Returns the count and spawn requests that may not yet be represented by a readback
    fn reserved_particle_count(&self) -> u64 {
        u64::from(self.particle_count.unwrap_or(0))
            .saturating_add(self.spawn_revision.saturating_sub(self.particle_count_revision))
            .saturating_add(self.queued_imports.len() as u64)
    }

}

/// Decodes world-cell records written by the particle compaction pass
pub(crate) fn decode_exported_particle_records(
    bytes: &[u8],
) -> Result<Vec<(ChunkCoordinates, ChunkFluidParticle)>, SceneError> {
    if bytes.len() < 4 { return Err(SceneError::FluidParticleExportRecordInvalid); }
    let count: usize = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
    let required_length: usize =
        4usize.saturating_add(count.saturating_mul(EXPORT_RECORD_BYTE_COUNT));
    if required_length > bytes.len() { return Err(SceneError::FluidParticleExportRecordInvalid); }
    let read_u32 = |offset: usize| -> u32 {
        u32::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ])
    };
    let mut particles: Vec<(ChunkCoordinates, ChunkFluidParticle)> = Vec::with_capacity(count);
    for particle_index in 0..count {
        let record_start: usize = 4 + particle_index * EXPORT_RECORD_BYTE_COUNT;
        let world_x: i64 = ((u64::from(read_u32(record_start + 4)) << 32)
            | u64::from(read_u32(record_start))) as i64;
        let world_y: i64 = ((u64::from(read_u32(record_start + 12)) << 32)
            | u64::from(read_u32(record_start + 8))) as i64;
        let fraction_x: f32 = f32::from_bits(read_u32(record_start + 16));
        let fraction_y: f32 = f32::from_bits(read_u32(record_start + 20));
        let velocity: [f32; 2] = [
            f32::from_bits(read_u32(record_start + 24)),
            f32::from_bits(read_u32(record_start + 28)),
        ];
        let material_identifier: MaterialIdentifier =
            MaterialIdentifier::from_u32(read_u32(record_start + 32));
        if !fraction_x.is_finite() || !(0.0..1.0).contains(&fraction_x)
            || !fraction_y.is_finite() || !(0.0..1.0).contains(&fraction_y)
        { return Err(SceneError::FluidParticleExportRecordInvalid); }
        let chunk_x: i32 = world_x.div_euclid(CHUNK_SIZE).try_into()
            .map_err(|_| SceneError::FluidParticleExportRecordInvalid)?;
        let chunk_y: i32 = world_y.div_euclid(CHUNK_SIZE).try_into()
            .map_err(|_| SceneError::FluidParticleExportRecordInvalid)?;
        let coordinates: ChunkCoordinates = ChunkCoordinates { x: chunk_x, y: chunk_y };
        let position: [f32; 2] = [
            world_x.rem_euclid(CHUNK_SIZE) as f32 + fraction_x,
            world_y.rem_euclid(CHUNK_SIZE) as f32 + fraction_y,
        ];
        let particle: ChunkFluidParticle = ChunkFluidParticle::new(
            position,
            velocity,
            material_identifier,
        ).map_err(|_| SceneError::FluidParticleExportRecordInvalid)?;
        particles.push((coordinates, particle));
    }
    Ok(particles)
}
