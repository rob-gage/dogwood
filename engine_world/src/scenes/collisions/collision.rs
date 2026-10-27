// Copyright Rob Gage 2026

use super::{
    collider::Collider,
    collider_shape::ColliderShape,
    collision_body::CollisionBody,
    collision_cellular_tile::CollisionCellularTile,
    collision_cellular_tile_pass::CollisionCellularTilePass,
    collision_rigid_body::CollisionRigidBody,
    collision_rigid_body_pass::CollisionRigidBodyPass,
    COLLISION_CONTACT_ITERATION_COUNT,
    COLLISION_GROUP_CELLULAR_DYNAMIC,
    COLLISION_GROUP_CELLULAR_TILE,
    COLLISION_GROUP_EDIT,
    COLLISION_GROUP_RIGID_BODY,
    COLLISION_ORIGIN_DISTANCE_MAXIMUM,
    COLLISION_RIGID_BODY_CELL_COUNT_MAXIMUM,
    COLLISION_RIGID_BODY_CELL_COUNT_MINIMUM,
    COLLISION_RIGID_BODY_CCD_PREDICTION,
    COLLISION_RIGID_BODY_CELLULAR_TILE_COUNT_MAXIMUM,
    COLLISION_RIGID_BODY_COUNT_MAXIMUM,
    COLLISION_RIGID_BODY_SOLVER_ITERATION_COUNT,
    COLLISION_STATIC_CONTACT_FREQUENCY,
};
use crate::{
    actors::{
        components::{
            ActorCollisionComponent,
            ActorContactComponent,
            ActorIdentityComponent,
            ActorKinematicsPositionComponent,
            ActorPawnControlComponent,
            ActorPawnMovementComponent,
            ActorPersistenceComponent,
        },
        Actor,
        ActorError,
        ActorRegistry,
    },
    materials::{
        MaterialIdentifier,
        MaterialTable,
    },
    scenes::{
        cellular_tiles::{
            CellularTileBuffers,
            CellularTileCoordinates,
            CellularTileRegion,
            CELLULAR_TILE_SIZE,
        },
        SceneError,
        ScenePosition,
        fluids::{
            FLUID_TIME_STEP,
            FluidActorContact,
        },
        pressure::{
            PRESSURE_BROKEN_CELLULAR_TILE_COUNT_MAXIMUM,
            PRESSURE_BROKEN_RIGID_BODY_CELL_COUNT_MAXIMUM,
        },
    },
};
use engine_host::acceleration::{Accelerator, AcceleratorReadback, AcceleratorReadbackError};
use std::collections::{
    hash_map::Entry,
    BTreeMap,
    HashMap,
    HashSet,
};

/// Fraction of a rigid body's wet-face fluid mass that drags it toward the current each tick
const RIGID_BODY_FLUID_CURRENT_COUPLING: f32 = 1.0;

/// Rapier simulation of a scene's actors and rigid bodies against the solid cells of its active
/// cellular tiles
pub struct Collision {
    /// Cellular tile whose bottom-left corner is the origin of every Rapier position
    origin: CellularTileCoordinates,
    /// Active region and occupancies held by the cellular tile collider, absent before a readback
    cellular_tiles: Option<(CellularTileRegion, Vec<CollisionCellularTile>)>,
    /// Ring offset of the active region when its occupancies were read
    ring_offset: (u32, u32),
    /// Whether edits changed the GPU cells since their occupancies were read
    is_cellular_tile_read_requested: bool,
    /// Number of solid cells held by the cellular tile collider
    solid_cell_count: u64,
    /// Fixed voxel collider of every solid cell in the active region, absent while none are solid
    cellular_tile_collider: Option<rapier2d::geometry::ColliderHandle>,
    /// Active region and settled dynamic occupancies held by their collider
    dynamic_cellular_tiles: Option<(CellularTileRegion, Vec<CollisionCellularTile>)>,
    /// Number of settled dynamic cells held by their collider
    dynamic_cell_count: u64,
    /// Ring offset of the active region when the held settled dynamic occupancies were copied
    dynamic_ring_offset: Option<(u32, u32)>,
    /// Fixed voxel collider of settled dynamic cells, interacting only with cellular rigid bodies
    dynamic_cellular_tile_collider: Option<rapier2d::geometry::ColliderHandle>,
    /// Body, colliders, position, translation, and pawn velocity, grounding, and held jump
    actors: HashMap<Actor, (
        rapier2d::dynamics::RigidBodyHandle,
        Option<(rapier2d::geometry::ColliderHandle, ActorCollisionComponent)>,
        Option<(rapier2d::geometry::ColliderHandle, ActorContactComponent)>,
        ScenePosition,
        rapier2d::math::Vector,
        Option<(rapier2d::math::Vector, bool, bool, bool)>,
    )>,
    /// Character controller moving every pawn against the colliders it collides with
    pawn_movement_controller: rapier2d::control::KinematicCharacterController,
    /// GPU extraction of active cellular tile occupancies
    cellular_tile_pass: CollisionCellularTilePass,
    /// Rigid bodies of static cells by identifier
    collision_rigid_bodies: HashMap<u64, CollisionRigidBody>,
    /// Identifier given to the next rigid body, never reused
    next_rigid_body_identifier: u64,
    /// Rigid body record indices held by no rigid body
    free_rigid_body_indices: Vec<u32>,
    /// Rigid body tiles held by no rigid body
    free_cellular_tile_indices: Vec<u32>,
    /// Rigid body and coordinates in it of every held rigid body tile
    cellular_tile_rigid_bodies: HashMap<u32, (u64, (i32, i32))>,
    /// Cells written before the next rasterization, with their new and expected materials
    rigid_body_cell_writes: Vec<[u32; 4]>,
    /// Static cell groups merged into one read request at the end of this collision tick
    detached_cell_groups: Vec<Vec<(u32, u32)>>,
    /// Rigid bodies whose cells changed since their last rasterization
    changed_rigid_bodies: HashSet<u64>,
    /// Rigid body tiles cleared by the next rasterization regardless of their body's motion
    cleared_cellular_tiles: Vec<u32>,
    /// Records of removed rigid bodies whose cells the next rasterization clears, and the record
    /// indices and tiles freed after it
    removed_rigid_bodies: Vec<(u32, [f32; 14])>,
    /// Rigid body tiles freed after the next rasterization
    removed_cellular_tiles: Vec<u32>,
    /// GPU rasterization of rigid body cells into the active cellular tiles
    rigid_body_pass: CollisionRigidBodyPass,
    /// Pressure breakage of static tiles copied for the next collision tick
    broken_cellular_tile_readbacks: AcceleratorReadback<(CellularTileRegion, (u32, u32))>,
    /// Pressure breakage of rigid body cells copied for the next collision tick
    broken_rigid_body_cell_readbacks: AcceleratorReadback<()>,
    /// Rigid bodies of every simulated actor
    rigid_bodies: rapier2d::dynamics::RigidBodySet,
    /// Cellular tile collider and every simulated actor collider
    colliders: rapier2d::geometry::ColliderSet,
    /// Fixed timestep and solver settings
    integration_parameters: rapier2d::dynamics::IntegrationParameters,
    /// Rapier step pipeline
    physics_pipeline: rapier2d::pipeline::PhysicsPipeline,
    /// Sleeping and awake body islands
    island_manager: rapier2d::dynamics::IslandManager,
    /// Broad-phase bounding volume hierarchy of every collider
    broad_phase: rapier2d::geometry::BroadPhaseBvh,
    /// Contact and intersection pairs of every collider
    narrow_phase: rapier2d::geometry::NarrowPhase,
    /// Impulse joints between bodies
    impulse_joints: rapier2d::dynamics::ImpulseJointSet,
    /// Multibody joints between bodies
    multibody_joints: rapier2d::dynamics::MultibodyJointSet,
    /// Continuous collision detection solver
    ccd_solver: rapier2d::dynamics::CCDSolver,
}

impl Collision {

    /// Creates an empty simulation reading occupancies from the cellular tile buffers
    pub fn new(accelerator: &Accelerator, cellular_tile_buffers: &CellularTileBuffers) -> Self {
        Self {
            origin: CellularTileCoordinates { x: 0, y: 0 },
            cellular_tiles: None,
            ring_offset: (0, 0),
            is_cellular_tile_read_requested: false,
            solid_cell_count: 0,
            cellular_tile_collider: None,
            dynamic_cellular_tiles: None,
            dynamic_cell_count: 0,
            dynamic_ring_offset: None,
            dynamic_cellular_tile_collider: None,
            actors: HashMap::new(),
            // round pawns roll over steps lower than their radius, so taller ledges are walls
            pawn_movement_controller: rapier2d::control::KinematicCharacterController {
                offset: rapier2d::control::CharacterLength::Absolute(0.01),
                max_slope_climb_angle: 50.0_f32.to_radians(),
                min_slope_slide_angle: 50.0_f32.to_radians(),
                snap_to_ground: Some(rapier2d::control::CharacterLength::Absolute(0.2)),
                ..rapier2d::control::KinematicCharacterController::default()
            },
            cellular_tile_pass: CollisionCellularTilePass::new(
                accelerator,
                cellular_tile_buffers,
            ),
            collision_rigid_bodies: HashMap::new(),
            next_rigid_body_identifier: 1,
            free_rigid_body_indices: (0..COLLISION_RIGID_BODY_COUNT_MAXIMUM as u32).rev().collect(),
            free_cellular_tile_indices: (0..COLLISION_RIGID_BODY_CELLULAR_TILE_COUNT_MAXIMUM as u32)
                .rev()
                .collect(),
            cellular_tile_rigid_bodies: HashMap::new(),
            rigid_body_cell_writes: Vec::new(),
            detached_cell_groups: Vec::new(),
            changed_rigid_bodies: HashSet::new(),
            cleared_cellular_tiles: Vec::new(),
            removed_rigid_bodies: Vec::new(),
            removed_cellular_tiles: Vec::new(),
            rigid_body_pass: CollisionRigidBodyPass::new(accelerator, cellular_tile_buffers),
            broken_cellular_tile_readbacks: AcceleratorReadback::new(
                accelerator,
                "`Collision.broken_cellular_tile_readbacks`",
                4 + PRESSURE_BROKEN_CELLULAR_TILE_COUNT_MAXIMUM * 12,
                3,
            ),
            broken_rigid_body_cell_readbacks: AcceleratorReadback::new(
                accelerator,
                "`Collision.broken_rigid_body_cell_readbacks`",
                4 + PRESSURE_BROKEN_RIGID_BODY_CELL_COUNT_MAXIMUM * 4,
                3,
            ),
            rigid_bodies: rapier2d::dynamics::RigidBodySet::new(),
            colliders: rapier2d::geometry::ColliderSet::new(),
            // resolve stack loads before a body penetrates beyond the voxel surface contacts
            integration_parameters: rapier2d::dynamics::IntegrationParameters {
                num_internal_pgs_iterations: COLLISION_CONTACT_ITERATION_COUNT,
                static_contact_softness: rapier2d::dynamics::SpringCoefficients {
                    natural_frequency: COLLISION_STATIC_CONTACT_FREQUENCY,
                    ..rapier2d::dynamics::SpringCoefficients::contact_static_defaults()
                },
                ..rapier2d::dynamics::IntegrationParameters::default()
            },
            physics_pipeline: rapier2d::pipeline::PhysicsPipeline::new(),
            island_manager: rapier2d::dynamics::IslandManager::new(),
            broad_phase: rapier2d::geometry::BroadPhaseBvh::new(),
            narrow_phase: rapier2d::geometry::NarrowPhase::new(),
            impulse_joints: rapier2d::dynamics::ImpulseJointSet::new(),
            multibody_joints: rapier2d::dynamics::MultibodyJointSet::new(),
            ccd_solver: rapier2d::dynamics::CCDSolver::new(),
        }
    }

    /// Synchronizes the active cellular tiles, rigid bodies, and actors, then advances one fixed
    /// step, moving pawns by the velocity change, contact normal, and contact depth in tiles of the
    /// dynamic cells touching them and by the fluid they are immersed in, pushing rigid bodies by
    /// the impulses of dynamic cells touching them and the fluid currents
    /// around them, and rasterizing the rigid bodies where they moved
    pub(crate) fn simulate_collisions(
        &mut self,
        accelerator: &Accelerator,
        active_region: CellularTileRegion,
        ring_offset: (u32, u32),
        gravity: f32,
        actor_registry: &mut ActorRegistry,
        cellular_contacts: &HashMap<Actor, ([f32; 2], [f32; 2], f32)>,
        rigid_body_contacts: &HashMap<u64, ([f32; 2], f32, [f32; 7])>,
        fluid_actor_contacts: &HashMap<Actor, FluidActorContact>,
        material_table: &MaterialTable,
    ) -> Result<(), SceneError> {
        if self.ring_offset != ring_offset
            || self.cellular_tiles.as_ref()
                .is_some_and(|(region, _)| *region != active_region)
        {
            self.detached_cell_groups.clear();
        }
        self.simulate_collisions_read_broken_cells(
            accelerator,
            active_region,
            ring_offset,
            material_table,
        )?;
        self.simulate_collisions_read_detached_cells(
            accelerator,
            active_region,
            ring_offset,
            material_table,
        )?;
        // static occupancies only need a readback when streaming moves the ring or edits write cells
        if !self.cellular_tiles.as_ref().is_some_and(|(region, _)| *region == active_region)
            || self.ring_offset != ring_offset
            || self.is_cellular_tile_read_requested
        {
            // edits may replace rigid body cells, which leave their bodies before static cells
            // are read so that replaced cells read as static cells
            if self.is_cellular_tile_read_requested {
                self.is_cellular_tile_read_requested = false;
                self.simulate_collisions_read_changed_cells(
                    accelerator,
                    active_region,
                    ring_offset,
                    material_table,
                )?;
            }
            // read this tick's GPU occupancy before simulating against it
            let cellular_tiles: Vec<CollisionCellularTile> = self.cellular_tile_pass
                .read_cellular_tiles(accelerator, active_region, ring_offset, false)?;
            // keep the origin near the active region so f32 positions keep sub-cell precision
            let center: CellularTileCoordinates = CellularTileCoordinates {
                x: active_region.bottom_left.x + active_region.width() as i64 / 2,
                y: active_region.bottom_left.y + active_region.height() as i64 / 2,
            };
            if self.cellular_tiles.is_none()
                || center.x.abs_diff(self.origin.x) > COLLISION_ORIGIN_DISTANCE_MAXIMUM
                || center.y.abs_diff(self.origin.y) > COLLISION_ORIGIN_DISTANCE_MAXIMUM
            { self.simulate_collisions_move_origin(center); }
            let changed_cells: Vec<rapier2d::parry::math::IVector> =
                self.simulate_collisions_apply_cellular_tiles(active_region, cellular_tiles, false);
            self.ring_offset = ring_offset;
            self.simulate_collisions_detach_rigid_bodies(
                active_region,
                ring_offset,
                &changed_cells,
            );
        }
        // rigid bodies step against the newest settled cells the GPU has finished copying,
        // normally those of the previous tick, and wait only when no snapshot of this region and
        // ring offset exists yet, so moving grains never stall a tick behind queued GPU work
        if !self.collision_rigid_bodies.is_empty() {
            let snapshot: Option<Vec<CollisionCellularTile>> = self.cellular_tile_pass
                .read_dynamic_cellular_tiles(accelerator, active_region, ring_offset)?;
            let has_current_snapshot: bool = self.dynamic_ring_offset == Some(ring_offset)
                && self.dynamic_cellular_tiles.as_ref()
                    .is_some_and(|(region, _)| *region == active_region);
            let cellular_tiles: Option<Vec<CollisionCellularTile>> = match snapshot {
                Some(cellular_tiles) => Some(cellular_tiles),
                None if !has_current_snapshot => Some(self.cellular_tile_pass
                    .read_cellular_tiles(accelerator, active_region, ring_offset, true)?),
                None => None,
            };
            if let Some(cellular_tiles) = cellular_tiles {
                self.simulate_collisions_apply_cellular_tiles(active_region, cellular_tiles, true);
                self.dynamic_ring_offset = Some(ring_offset);
            }
            self.cellular_tile_pass
                .request_dynamic_cellular_tiles(accelerator, active_region, ring_offset)?;
        }
        // rigid bodies that left the active region stay behind as static cells
        self.release_rigid_bodies(accelerator, active_region, ring_offset, active_region);
        // impulses apply once and fluid damping lasts until newer contacts arrive
        for (rigid_body, (impulse, angular_impulse, fluid_current)) in rigid_body_contacts {
            let Some(collision_rigid_body) = self.collision_rigid_bodies.get_mut(rigid_body)
            else { continue; };
            let Some(rigid_body) = self.rigid_bodies.get_mut(collision_rigid_body.rigid_body_handle)
            else { continue; };
            if *impulse != [0.0; 2] || *angular_impulse != 0.0 {
                rigid_body.apply_impulse(rapier2d::math::Vector::new(impulse[0], impulse[1]), true);
                rigid_body.apply_torque_impulse(*angular_impulse, true);
            }
            // relax toward the wet faces' current with this body's newest velocity, exactly
            // through an exponential, so a late sample of the current cannot overshoot
            let fluid_mass: f32 = fluid_current[0];
            if fluid_mass > 0.0 {
                let mass: f32 = rigid_body.mass().max(f32::EPSILON);
                let velocity: rapier2d::math::Vector = rigid_body.linvel();
                let current: rapier2d::math::Vector =
                    rapier2d::math::Vector::new(fluid_current[1], fluid_current[2]) / fluid_mass;
                let linear_fraction: f32 =
                    1.0 - (-RIGID_BODY_FLUID_CURRENT_COUPLING * fluid_mass / mass).exp();
                rigid_body.apply_impulse((current - velocity) * (mass * linear_fraction), true);
                // the faces' spin about the center, less what the body's own motion explains
                let arm_moment: f32 = fluid_current[4];
                let inertia: f32 = rigid_body.mass_properties().local_mprops.principal_inertia();
                if arm_moment > 0.0 && inertia > 0.0 {
                    let arm_sum: rapier2d::math::Vector =
                        rapier2d::math::Vector::new(fluid_current[5], fluid_current[6]);
                    let current_spin: f32 = (fluid_current[3]
                        - (arm_sum.x * velocity.y - arm_sum.y * velocity.x)) / arm_moment;
                    let angular_fraction: f32 =
                        1.0 - (-RIGID_BODY_FLUID_CURRENT_COUPLING * arm_moment / inertia).exp();
                    rigid_body.apply_torque_impulse(
                        (current_spin - rigid_body.angvel()) * inertia * angular_fraction,
                        true,
                    );
                }
            }
        }
        self.simulate_collisions_step_actors(
            active_region,
            gravity,
            actor_registry,
            cellular_contacts,
            fluid_actor_contacts,
        )?;
        self.simulate_collisions_rasterize_rigid_bodies(accelerator, active_region, ring_offset);
        self.simulate_collisions_request_detached_cells(accelerator, active_region, ring_offset);
        Ok(())
    }

    /// Requests that the next simulation read the GPU cells after edits wrote them
    pub fn request_cellular_tile_read(&mut self) { self.is_cellular_tile_read_requested = true; }

    /// Reads pressure breakage from its prior tick and applies it to collision state
    fn simulate_collisions_read_broken_cells(
        &mut self,
        accelerator: &Accelerator,
        active_region: CellularTileRegion,
        ring_offset: (u32, u32),
        material_table: &MaterialTable,
    ) -> Result<(), SceneError> {
        let mut broken_tiles: Option<((CellularTileRegion, (u32, u32)), Vec<u8>)> = None;
        let tile_read_result: Result<(), AcceleratorReadbackError> =
            self.broken_cellular_tile_readbacks.read(accelerator, true, |metadata, bytes| {
                broken_tiles = Some((metadata, bytes.to_vec()));
            });
        tile_read_result.map_err(|error| match error {
            AcceleratorReadbackError::Poll(_) => SceneError::CollisionCellularTilePoll,
            AcceleratorReadbackError::BufferAsync(error) =>
                SceneError::CollisionCellularTileResponse { error },
        })?;
        let mut broken_rigid_body_cells: Vec<u32> = Vec::new();
        let rigid_body_read_result: Result<(), AcceleratorReadbackError> =
            self.broken_rigid_body_cell_readbacks.read(accelerator, true, |(), bytes| {
                let count: usize = u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as usize;
                broken_rigid_body_cells.extend(bytes[4..].chunks_exact(4).take(count).map(|cell| {
                    u32::from_le_bytes(cell.try_into().unwrap())
                }));
            });
        rigid_body_read_result.map_err(|error| match error {
            AcceleratorReadbackError::Poll(_) => SceneError::CollisionCellularTilePoll,
            AcceleratorReadbackError::BufferAsync(error) =>
                SceneError::CollisionCellularTileResponse { error },
        })?;
        if !broken_rigid_body_cells.is_empty() {
            self.simulate_collisions_remove_rigid_body_cells(
                accelerator,
                &broken_rigid_body_cells,
                material_table,
            );
        }
        let Some(((read_region, read_ring_offset), bytes)) = broken_tiles else { return Ok(()); };
        let count: usize = u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as usize;
        if count > PRESSURE_BROKEN_CELLULAR_TILE_COUNT_MAXIMUM as usize {
            self.is_cellular_tile_read_requested = true;
            return Ok(());
        }
        if read_region != active_region || read_ring_offset != ring_offset { return Ok(()); }
        let Some((region, previous_tiles)) = self.cellular_tiles.as_ref() else { return Ok(()); };
        if *region != active_region { return Ok(()); }
        let mut cellular_tiles: Vec<CollisionCellularTile> = previous_tiles.clone();
        let width: usize = active_region.width();
        let height: usize = active_region.height();
        for tile in bytes[4..].chunks_exact(12).take(count) {
            let physical_index: usize = u32::from_le_bytes(tile[0..4].try_into().unwrap()) as usize;
            let physical_x: usize = physical_index % width;
            let physical_y: usize = physical_index / width;
            let logical_x: usize = (physical_x + width - ring_offset.0 as usize) % width;
            let logical_y: usize = (physical_y + height - ring_offset.1 as usize) % height;
            let occupancy: u64 = u64::from(u32::from_le_bytes(tile[4..8].try_into().unwrap()))
                | (u64::from(u32::from_le_bytes(tile[8..12].try_into().unwrap())) << 32);
            cellular_tiles[logical_y * width + logical_x] =
                CollisionCellularTile::from_u64(occupancy);
        }
        let changed_cells: Vec<rapier2d::parry::math::IVector> =
            self.simulate_collisions_apply_cellular_tiles(active_region, cellular_tiles, false);
        self.simulate_collisions_detach_rigid_bodies(
            active_region,
            ring_offset,
            &changed_cells,
        );
        Ok(())
    }

    /// Copies pressure-broken cells for the next collision tick without waiting for the GPU
    pub fn request_broken_cells(
        &mut self,
        accelerator: &Accelerator,
        active_region: CellularTileRegion,
        ring_offset: (u32, u32),
        broken_cellular_tile_buffer: &wgpu::Buffer,
        broken_rigid_body_cell_buffer: &wgpu::Buffer,
    ) {
        let mut command_encoder: wgpu::CommandEncoder = accelerator.device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Scene collision pressure breakage readbacks"),
            });
        self.broken_cellular_tile_readbacks.copy(
            &mut command_encoder,
            broken_cellular_tile_buffer,
            0,
            broken_cellular_tile_buffer.size(),
            (active_region, ring_offset),
        );
        self.broken_rigid_body_cell_readbacks.copy(
            &mut command_encoder,
            broken_rigid_body_cell_buffer,
            0,
            broken_rigid_body_cell_buffer.size(),
            (),
        );
        let submission_index: wgpu::SubmissionIndex =
            accelerator.queue().submit(Some(command_encoder.finish()));
        self.broken_cellular_tile_readbacks.map(submission_index.clone());
        self.broken_rigid_body_cell_readbacks.map(submission_index);
    }

    /// Returns the cells of cellular tiles overlapped by colliders in the edit collision group
    pub fn intersect_collisions(
        actor_registry: &ActorRegistry,
        cellular_tiles: &HashSet<CellularTileCoordinates>,
    ) -> Result<HashMap<CellularTileCoordinates, u64>, SceneError> {
        // shrink each cell slightly so colliders only touching it do not overlap it
        let cell_size: f32 = 1.0 / CELLULAR_TILE_SIZE as f32;
        let cell: rapier2d::parry::shape::Cuboid = rapier2d::parry::shape::Cuboid::new(
            rapier2d::math::Vector::splat(cell_size / 2.0 - 1.0e-4),
        );
        let mut intersections: HashMap<CellularTileCoordinates, u64> = HashMap::new();
        for (actor_identity, position) in actor_registry
            .components::<ActorIdentityComponent, ActorKinematicsPositionComponent>()
        {
            let actor: Actor = actor_identity.actor();
            let Ok(collision) = actor_registry.component::<ActorCollisionComponent>(actor)
            else { continue; };
            if collision.memberships & COLLISION_GROUP_EDIT == 0 { continue; }
            // place the origin at the actor's tile so nearby cells are located exactly
            let position: ScenePosition = position.position;
            let translation: rapier2d::math::Vector = Self::rapier_translation(
                CellularTileCoordinates { x: position.tile_x, y: position.tile_y },
                position,
            );
            let bounds: rapier2d::parry::bounding_volume::Aabb =
                Self::simulate_collisions_collider_bounds(actor, &collision.collider, translation)?;
            let shape: rapier2d::geometry::SharedShape = Self::rapier_shape(&collision.collider);
            let pose: rapier2d::math::Pose = rapier2d::math::Pose::new(
                translation + rapier2d::math::Vector::new(
                    collision.collider.offset_x,
                    collision.collider.offset_y,
                ),
                collision.collider.rotation,
            );
            for coordinates in cellular_tiles {
                let tile_x: f32 = coordinates.x.saturating_sub(position.tile_x) as f32;
                let tile_y: f32 = coordinates.y.saturating_sub(position.tile_y) as f32;
                if tile_x + 1.0 < bounds.mins.x || tile_x > bounds.maxs.x
                    || tile_y + 1.0 < bounds.mins.y || tile_y > bounds.maxs.y
                { continue; }
                let mut cells: u64 = 0;
                for cell_index in 0..64 {
                    let cell_center: rapier2d::math::Vector = rapier2d::math::Vector::new(
                        tile_x + ((cell_index % 8) as f32 + 0.5) * cell_size,
                        tile_y + ((cell_index / 8) as f32 + 0.5) * cell_size,
                    );
                    let is_intersecting: bool = matches!(
                        rapier2d::parry::query::intersection_test(
                            &pose,
                            &*shape,
                            &rapier2d::math::Pose::new(cell_center, 0.0),
                            &cell,
                        ),
                        Ok(true),
                    );
                    if is_intersecting { cells |= 1 << cell_index; }
                }
                if cells != 0 { *intersections.entry(*coordinates).or_insert(0) |= cells; }
            }
        }
        Ok(intersections)
    }

    /// Moves the origin, removing the cellular tile collider and re-placing every body
    fn simulate_collisions_move_origin(&mut self, origin: CellularTileCoordinates) {
        // rigid bodies keep their place relative to the cells, shifted by the whole tiles moved
        let shift: rapier2d::math::Vector = rapier2d::math::Vector::new(
            (self.origin.x - origin.x) as f32,
            (self.origin.y - origin.y) as f32,
        );
        for collision_rigid_body in self.collision_rigid_bodies.values_mut() {
            if let Some(rigid_body) =
                self.rigid_bodies.get_mut(collision_rigid_body.rigid_body_handle)
            {
                rigid_body.set_translation(rigid_body.translation() + shift, false);
            }
            if let Some((position, ..)) = collision_rigid_body.rasterization.as_mut() {
                position[0] += shift.x * CELLULAR_TILE_SIZE as f32;
                position[1] += shift.y * CELLULAR_TILE_SIZE as f32;
            }
        }
        self.origin = origin;
        if let Some(cellular_tile_collider) = self.cellular_tile_collider.take() {
            self.colliders.remove(
                cellular_tile_collider,
                &mut self.island_manager,
                &mut self.rigid_bodies,
                false,
            );
        }
        if let Some(collider) = self.dynamic_cellular_tile_collider.take() {
            self.colliders.remove(
                collider,
                &mut self.island_manager,
                &mut self.rigid_bodies,
                false,
            );
        }
        self.dynamic_cellular_tiles = None;
        self.dynamic_cell_count = 0;
        self.cellular_tiles = None;
        self.solid_cell_count = 0;
        // derive each translation from its exact position so moving the origin never drifts
        for (rigid_body_handle, _, _, position, translation, _) in self.actors.values_mut() {
            *translation = Self::rapier_translation(origin, *position);
            if let Some(rigid_body) = self.rigid_bodies.get_mut(*rigid_body_handle) {
                rigid_body.set_translation(*translation, true);
                if rigid_body.is_kinematic() {
                    rigid_body.set_next_kinematic_translation(*translation);
                }
            }
        }
    }

    /// Applies cells that changed, entered, or left the active region to the voxel collider,
    /// returning the voxel keys of the cells that changed or left rather than entered
    fn simulate_collisions_apply_cellular_tiles(
        &mut self,
        active_region: CellularTileRegion,
        cellular_tiles: Vec<CollisionCellularTile>,
        dynamic: bool,
    ) -> Vec<rapier2d::parry::math::IVector> {
        let previous_cellular_tiles: Option<(
            CellularTileRegion,
            Vec<CollisionCellularTile>,
        )> = if dynamic { self.dynamic_cellular_tiles.take() } else { self.cellular_tiles.take() };
        let mut solid_cell_count: u64 = if dynamic { self.dynamic_cell_count } else {
            self.solid_cell_count
        };
        let mut cellular_tile_collider: Option<rapier2d::geometry::ColliderHandle> =
            if dynamic { self.dynamic_cellular_tile_collider } else { self.cellular_tile_collider };
        let mut cell_changes: Vec<(rapier2d::parry::math::IVector, bool)> = Vec::new();
        // compare every previous tile with its current occupancy, which is empty once it leaves
        if let Some((previous_region, previous_tiles)) = &previous_cellular_tiles {
            for (coordinates, previous_tile) in
                previous_region.cellular_tile_coordinates().zip(previous_tiles)
            {
                let occupancy: u64 = if active_region.contains(coordinates) {
                    let logical_x: usize = (coordinates.x - active_region.bottom_left.x) as usize;
                    let logical_y: usize = (coordinates.y - active_region.bottom_left.y) as usize;
                    cellular_tiles[logical_y * active_region.width() + logical_x].as_u64()
                } else { 0 };
                self.simulate_collisions_collect_cell_changes(
                    &mut cell_changes,
                    coordinates,
                    previous_tile.as_u64() ^ occupancy,
                    occupancy,
                );
            }
        }
        let changed_cell_count: usize = cell_changes.len();
        // fill the solid cells of tiles entering the active region
        for (coordinates, cellular_tile) in
            active_region.cellular_tile_coordinates().zip(&cellular_tiles)
        {
            if previous_cellular_tiles.as_ref()
                .is_some_and(|(previous_region, _)| previous_region.contains(coordinates))
            { continue; }
            self.simulate_collisions_collect_cell_changes(
                &mut cell_changes,
                coordinates,
                cellular_tile.as_u64(),
                cellular_tile.as_u64(),
            );
        }
        if dynamic { self.dynamic_cellular_tiles = Some((active_region, cellular_tiles)); } else {
            self.cellular_tiles = Some((active_region, cellular_tiles));
        }
        // tiles that were already active are compared before entering tiles are filled
        let changed_cells: Vec<rapier2d::parry::math::IVector> = cell_changes.iter()
            .take(changed_cell_count)
            .map(|(key, _)| *key)
            .collect();
        if cell_changes.is_empty() { return changed_cells; }
        for (_, is_filled) in &cell_changes {
            if *is_filled { solid_cell_count += 1; } else { solid_cell_count -= 1; }
        }
        // edit the existing voxels in place, creating the collider once a cell becomes solid
        match cellular_tile_collider
            .and_then(|cellular_tile_collider| self.colliders.get_mut(cellular_tile_collider))
        {
            Some(collider) => {
                if let Some(voxels) = collider.shape_mut().as_voxels_mut() {
                    for (key, is_filled) in cell_changes {
                        voxels.set_voxel(key, is_filled);
                    }
                }
            }
            None => {
                // filled one voxel at a time because Parry 0.30's bulk-built chunk hierarchy can
                // panic when a chunk is later emptied
                let cell_size: f32 = 1.0 / CELLULAR_TILE_SIZE as f32;
                let mut voxels: rapier2d::parry::shape::Voxels =
                    rapier2d::parry::shape::Voxels::new(
                        rapier2d::math::Vector::splat(cell_size),
                        &[],
                    );
                for (key, is_filled) in cell_changes {
                    voxels.set_voxel(key, is_filled);
                }
                let collider: rapier2d::geometry::Collider =
                    rapier2d::geometry::ColliderBuilder::new(
                        rapier2d::geometry::SharedShape::new(voxels),
                    )
                        .collision_groups(rapier2d::geometry::InteractionGroups::new(
                            rapier2d::geometry::Group::from_bits_retain(
                                if dynamic { COLLISION_GROUP_CELLULAR_DYNAMIC } else {
                                    COLLISION_GROUP_CELLULAR_TILE
                                },
                            ),
                            if dynamic {
                                rapier2d::geometry::Group::from_bits_retain(COLLISION_GROUP_RIGID_BODY)
                            } else { rapier2d::geometry::Group::ALL },
                            rapier2d::geometry::InteractionTestMode::And,
                        ))
                        .build();
                cellular_tile_collider = Some(self.colliders.insert(collider));
            }
        }
        if solid_cell_count == 0 && let Some(cellular_tile_collider) =
            cellular_tile_collider.take()
        {
            self.colliders.remove(
                cellular_tile_collider,
                &mut self.island_manager,
                &mut self.rigid_bodies,
                false,
            );
        }
        if dynamic {
            self.dynamic_cell_count = solid_cell_count;
            self.dynamic_cellular_tile_collider = cellular_tile_collider;
        } else {
            self.solid_cell_count = solid_cell_count;
            self.cellular_tile_collider = cellular_tile_collider;
        }
        changed_cells
    }

    /// Records the voxel key and fill of every changed cell in one cellular tile
    fn simulate_collisions_collect_cell_changes(
        &self,
        cell_changes: &mut Vec<(rapier2d::parry::math::IVector, bool)>,
        coordinates: CellularTileCoordinates,
        mut changed_cells: u64,
        occupancy: u64,
    ) {
        let cell_x: i64 = (coordinates.x - self.origin.x) * CELLULAR_TILE_SIZE;
        let cell_y: i64 = (coordinates.y - self.origin.y) * CELLULAR_TILE_SIZE;
        while changed_cells != 0 {
            let cell_index: u32 = changed_cells.trailing_zeros();
            changed_cells &= changed_cells - 1;
            let key: rapier2d::parry::math::IVector = rapier2d::parry::math::IVector::new(
                (cell_x + i64::from(cell_index) % CELLULAR_TILE_SIZE) as i32,
                (cell_y + i64::from(cell_index) / CELLULAR_TILE_SIZE) as i32,
            );
            cell_changes.push((key, occupancy >> cell_index & 1 == 1));
        }
    }

    /// Synchronizes actor bodies, applies simulation exits, steps, and writes back moved actors
    fn simulate_collisions_step_actors(
        &mut self,
        active_region: CellularTileRegion,
        gravity: f32,
        actor_registry: &mut ActorRegistry,
        cellular_contacts: &HashMap<Actor, ([f32; 2], [f32; 2], f32)>,
        fluid_actor_contacts: &HashMap<Actor, FluidActorContact>,
    ) -> Result<(), SceneError> {
        let region_minimum: rapier2d::math::Vector = rapier2d::math::Vector::new(
            (active_region.bottom_left.x - self.origin.x) as f32,
            (active_region.bottom_left.y - self.origin.y) as f32,
        );
        let region_maximum: rapier2d::math::Vector = rapier2d::math::Vector::new(
            (active_region.top_right.x + 1 - self.origin.x) as f32,
            (active_region.top_right.y + 1 - self.origin.y) as f32,
        );
        let actor_positions: Vec<(Actor, ScenePosition)> = actor_registry
            .components::<ActorIdentityComponent, ActorKinematicsPositionComponent>()
            .map(|(actor_identity, position)| (actor_identity.actor(), position.position))
            .collect();
        let mut exiting_actors: Vec<Actor> = Vec::new();
        let mut simulated_actors: HashSet<Actor> = HashSet::new();
        let mut pawns: Vec<(Actor, ActorPawnMovementComponent, ActorPawnControlComponent)> =
            Vec::new();
        for (actor, position) in actor_positions {
            let collision: Option<ActorCollisionComponent> = actor_registry
                .component::<ActorCollisionComponent>(actor)
                .ok()
                .cloned();
            let contact: Option<ActorContactComponent> = actor_registry
                .component::<ActorContactComponent>(actor)
                .ok()
                .cloned();
            let translation: rapier2d::math::Vector =
                Self::rapier_translation(self.origin, position);
            // an actor exits once any collider, or its position without colliders, leaves
            let mut is_inside: bool = translation.cmpge(region_minimum).all()
                && translation.cmplt(region_maximum).all();
            for collider in collision.iter().map(|collision| &collision.collider)
                .chain(contact.iter().map(|contact| &contact.collider))
            {
                let collider_bounds: rapier2d::parry::bounding_volume::Aabb =
                    Self::simulate_collisions_collider_bounds(actor, collider, translation)?;
                is_inside = collider_bounds.mins.cmpge(region_minimum).all()
                    && collider_bounds.maxs.cmple(region_maximum).all();
                if !is_inside { break; }
            }
            if !is_inside {
                exiting_actors.push(actor);
                continue;
            }
            if collision.is_none() && contact.is_none() { continue; }
            simulated_actors.insert(actor);
            // only kinematic actors with a collision collider are moved as pawns
            let pawn_movement: Option<ActorPawnMovementComponent> = actor_registry
                .component::<ActorPawnMovementComponent>(actor)
                .ok()
                .copied()
                .filter(|_| {
                    collision.as_ref()
                        .is_some_and(|collision| collision.body == CollisionBody::Kinematic)
                });
            if let Some(pawn_movement) = pawn_movement {
                let pawn_control: ActorPawnControlComponent = actor_registry
                    .component::<ActorPawnControlComponent>(actor)
                    .copied()
                    .unwrap_or_default();
                pawns.push((actor, pawn_movement, pawn_control));
            }
            self.simulate_collisions_update_actor(
                actor,
                position,
                translation,
                collision,
                contact,
                pawn_movement.is_some(),
            );
            if let Some(fluid_contact) = fluid_actor_contacts.get(&actor) {
                self.simulate_collisions_apply_fluid_actor_contact(actor, *fluid_contact, gravity);
            }
        }
        // remove the bodies of actors that despawned, exited, or lost both colliders
        let removed_actors: Vec<Actor> = self.actors.keys()
            .filter(|actor| !simulated_actors.contains(actor))
            .copied()
            .collect();
        for actor in removed_actors {
            let Some((rigid_body_handle, _, _, _, _, _)) = self.actors.remove(&actor)
            else { continue; };
            self.rigid_bodies.remove(
                rigid_body_handle,
                &mut self.island_manager,
                &mut self.colliders,
                &mut self.impulse_joints,
                &mut self.multibody_joints,
                true,
            );
        }
        // exiting actors despawn unless they persist
        for actor in exiting_actors {
            if actor_registry.component::<ActorPersistenceComponent>(actor).is_ok() { continue; }
            actor_registry.despawn(actor)
                .map_err(|error: ActorError| SceneError::ActorDespawn { error })?;
        }
        self.simulate_collisions_move_pawns(
            &pawns,
            gravity,
            cellular_contacts,
            fluid_actor_contacts,
        );
        self.physics_pipeline.step(
            rapier2d::math::Vector::new(0.0, -gravity),
            &self.integration_parameters,
            &mut self.island_manager,
            &mut self.broad_phase,
            &mut self.narrow_phase,
            &mut self.rigid_bodies,
            &mut self.colliders,
            &mut self.impulse_joints,
            &mut self.multibody_joints,
            &mut self.ccd_solver,
            &(),
            &(),
        );
        // write the exact positions of dynamic bodies and pawns that moved back to their actors
        let origin: CellularTileCoordinates = self.origin;
        for (actor, (rigid_body_handle, _, _, position, translation, _)) in &mut self.actors {
            let Some(rigid_body) = self.rigid_bodies.get(*rigid_body_handle) else { continue; };
            if rigid_body.translation() == *translation { continue; }
            *translation = rigid_body.translation();
            let x: f64 = f64::from(translation.x);
            let y: f64 = f64::from(translation.y);
            *position = ScenePosition {
                tile_x: origin.x + x.floor() as i64,
                tile_y: origin.y + y.floor() as i64,
                offset_x: ((x - x.floor()) * ScenePosition::OFFSET_UNITS_PER_TILE) as u32,
                offset_y: ((y - y.floor()) * ScenePosition::OFFSET_UNITS_PER_TILE) as u32,
            };
            if let Ok(actor_position) =
                actor_registry.component_mutable::<ActorKinematicsPositionComponent>(*actor)
            { actor_position.position = *position; }
        }
        Ok(())
    }

    /// Applies fluid impulses and buoyancy to dynamic actors using the latest asynchronous sample
    fn simulate_collisions_apply_fluid_actor_contact(
        &mut self,
        actor: Actor,
        contact: FluidActorContact,
        gravity: f32,
    ) {
        let Some((rigid_body_handle, Some((_, collision)), _, _, _, _)) = self.actors.get(&actor)
        else { return; };
        if collision.body != CollisionBody::Dynamic { return; }
        let area_in_cells: f32 = Self::simulate_collisions_actor_area_in_cells(&collision.collider);
        let actor_density: f32 = collision.mass / area_in_cells;
        let buoyancy_ratio: f32 = contact.immersion * contact.fluid_density / actor_density;
        let cells_per_tick_to_tiles_per_second: f32 =
            1.0 / (CELLULAR_TILE_SIZE as f32 * FLUID_TIME_STEP);
        let impulse: rapier2d::math::Vector = rapier2d::math::Vector::new(
            contact.impulse[0] * cells_per_tick_to_tiles_per_second,
            contact.impulse[1] * cells_per_tick_to_tiles_per_second
                + collision.mass * gravity * buoyancy_ratio * self.integration_parameters.dt,
        );
        if let Some(rigid_body) = self.rigid_bodies.get_mut(*rigid_body_handle) {
            rigid_body.apply_impulse(impulse, true);
        }
    }

    /// Returns the collider's area in fluid cells, using one tile as eight cells
    fn simulate_collisions_actor_area_in_cells(collider: &Collider) -> f32 {
        let tile_area: f32 = match collider.shape {
            ColliderShape::Circle { radius } => std::f32::consts::PI * radius * radius,
            ColliderShape::Rectangle { width, height } => width * height,
            ColliderShape::Capsule { width, height } => {
                width * (height - width)
                    + std::f32::consts::PI * (width * 0.5) * (width * 0.5)
            }
        };
        (CELLULAR_TILE_SIZE * CELLULAR_TILE_SIZE) as f32 * tile_area
    }

    /// Moves every pawn by its controlled velocity, gravity, fluid sample, and dynamic cell
    /// contacts with the character controller
    fn simulate_collisions_move_pawns(
        &mut self,
        pawns: &[(Actor, ActorPawnMovementComponent, ActorPawnControlComponent)],
        gravity: f32,
        cellular_contacts: &HashMap<Actor, ([f32; 2], [f32; 2], f32)>,
        fluid_actor_contacts: &HashMap<Actor, FluidActorContact>,
    ) {
        let time_step: f32 = self.integration_parameters.dt;
        let maximum_slope_cosine: f32 = self.pawn_movement_controller.max_slope_climb_angle.cos();
        for (actor, pawn_movement, pawn_control) in pawns {
            // dynamic cells push the pawn by the momentum of the cells it shoved, and hold it like
            // ground where they could not get out of its way
            let (velocity_change, cellular_normal, cellular_depth): ([f32; 2], [f32; 2], f32) =
                cellular_contacts.get(actor).copied().unwrap_or(([0.0; 2], [0.0; 2], 0.0));
            let cellular_normal: rapier2d::math::Vector =
                rapier2d::math::Vector::new(cellular_normal[0], cellular_normal[1]);
            let is_cellular_grounded: bool = cellular_normal.y >= maximum_slope_cosine;
            let fluid_contact: FluidActorContact =
                fluid_actor_contacts.get(actor).copied().unwrap_or_default();
            let Some((
                rigid_body_handle,
                Some((collider_handle, _)),
                _,
                _,
                _,
                Some(pawn),
            )) = self.actors.get_mut(actor)
            else { continue; };
            let (velocity, is_grounded, is_jump_held, is_swimming): &mut (
                rapier2d::math::Vector,
                bool,
                bool,
                bool,
            ) = pawn;
            *velocity += rapier2d::math::Vector::new(velocity_change[0], velocity_change[1]);
            let is_standing: bool = *is_grounded || is_cellular_grounded;
            let immersion: f32 = fluid_contact.immersion.clamp(0.0, 1.0);
            let swimming_enter_immersion: f32 =
                pawn_movement.swimming_enter_immersion.clamp(0.0, 1.0);
            let swimming_exit_immersion: f32 = pawn_movement.swimming_exit_immersion
                .clamp(0.0, swimming_enter_immersion);
            *is_swimming = if *is_swimming {
                immersion >= swimming_exit_immersion
            } else {
                immersion >= swimming_enter_immersion
            };
            let (Some(rigid_body), Some(collider)) = (
                self.rigid_bodies.get(*rigid_body_handle),
                self.colliders.get(*collider_handle),
            ) else { continue; };
            let velocity_change: f32 = pawn_movement.acceleration * time_step;
            if *is_swimming {
                let cells_per_tick_to_tiles_per_second: f32 =
                    1.0 / (CELLULAR_TILE_SIZE as f32 * FLUID_TIME_STEP);
                let fluid_velocity: rapier2d::math::Vector = rapier2d::math::Vector::new(
                    fluid_contact.fluid_velocity[0] * cells_per_tick_to_tiles_per_second,
                    fluid_contact.fluid_velocity[1] * cells_per_tick_to_tiles_per_second,
                );
                let buoyancy_ratio: f32 = immersion * fluid_contact.fluid_density
                    / pawn_movement.swimming_density.max(0.000001);
                velocity.y -= gravity * (1.0 - buoyancy_ratio) * time_step;
                let drag: f32 = (-fluid_contact.fluid_drag.max(0.0)
                    * pawn_movement.swimming_drag.max(0.0)
                    * immersion
                    * time_step).exp();
                let mut relative_velocity: rapier2d::math::Vector =
                    (*velocity - fluid_velocity) * drag;
                let vertical_control: f32 = if pawn_control.is_jumping {
                    1.0
                } else if pawn_control.is_crouching {
                    -1.0
                } else { 0.0 };
                let mut thrust: rapier2d::math::Vector = rapier2d::math::Vector::new(
                    pawn_control.movement.clamp(-1.0, 1.0),
                    vertical_control,
                );
                if thrust.length_squared() > 1.0 { thrust = thrust.normalize(); }
                relative_velocity += thrust
                    * pawn_movement.swimming_acceleration.max(0.0)
                    * time_step;
                let speed: f32 = relative_velocity.length();
                let maximum_speed: f32 = pawn_movement.swimming_maximum_speed.max(0.0);
                if speed > maximum_speed && speed > 0.0 {
                    relative_velocity *= maximum_speed / speed;
                }
                *velocity = fluid_velocity + relative_velocity;
                // the sampled water includes what the pawn itself shoves aside, so its world speed
                // stays within the faster of its stroke and the current
                let world_maximum_speed: f32 = maximum_speed.max(fluid_velocity.length());
                let world_speed: f32 = velocity.length();
                if world_speed > world_maximum_speed && world_speed > 0.0 {
                    *velocity *= world_maximum_speed / world_speed;
                }
            } else {
                // walking retains its platform speed and edge-triggered grounded jump
                let speed: f32 = if pawn_control.is_crouching {
                    pawn_movement.crouching_speed
                } else {
                    pawn_movement.speed
                };
                let target_velocity_x: f32 = pawn_control.movement.clamp(-1.0, 1.0) * speed;
                velocity.x += (target_velocity_x - velocity.x)
                    .clamp(-velocity_change, velocity_change);
                if pawn_control.is_jumping && !*is_jump_held && is_standing {
                    velocity.y = pawn_movement.jump_speed;
                }
                if is_standing && velocity.y <= 0.0 {
                    velocity.y = 0.0;
                } else {
                    velocity.y -= gravity * time_step;
                }
            }
            *is_jump_held = pawn_control.is_jumping;
            let cellular_approach_speed: f32 = velocity.dot(cellular_normal);
            if cellular_approach_speed < 0.0 {
                *velocity -= cellular_normal * cellular_approach_speed;
            }
            // cast from the body's next pose so gameplay placement this tick is respected
            let collider_pose: rapier2d::math::Pose = *rigid_body.next_position()
                * collider.position_wrt_parent().copied().unwrap_or_default();
            // pawns sink one cell into dynamic cells and are pushed out of anything deeper
            let cellular_correction: f32 =
                (cellular_depth - 1.0 / CELLULAR_TILE_SIZE as f32).max(0.0) / 2.0;
            let desired_translation: rapier2d::math::Vector =
                *velocity * time_step + cellular_normal * cellular_correction;
            let mut resolved_velocity: rapier2d::math::Vector = *velocity;
            let movement: rapier2d::control::EffectiveCharacterMovement =
                self.pawn_movement_controller.move_shape(
                    time_step,
                    &self.broad_phase.as_query_pipeline(
                        self.narrow_phase.query_dispatcher(),
                        &self.rigid_bodies,
                        &self.colliders,
                        rapier2d::pipeline::QueryFilter::default()
                            .exclude_rigid_body(*rigid_body_handle)
                            .exclude_sensors()
                            .groups(collider.collision_groups()),
                    ),
                    collider.shape(),
                    &collider_pose,
                    desired_translation,
                    // each surface hit removes only the velocity pushing into it
                    |character_collision: rapier2d::control::CharacterCollision| {
                        let normal: rapier2d::math::Vector = character_collision.hit.normal1;
                        let approach_speed: f32 = resolved_velocity.dot(normal);
                        if approach_speed < 0.0 { resolved_velocity -= normal * approach_speed; }
                    },
                );
            // snapping to the ground stops a fall without reporting a hit
            if movement.grounded && resolved_velocity.y < 0.0 { resolved_velocity.y = 0.0; }
            *velocity = resolved_velocity;
            *is_grounded = movement.grounded || is_cellular_grounded;
            let next_translation: rapier2d::math::Vector =
                rigid_body.next_position().translation + movement.translation;
            if let Some(rigid_body) = self.rigid_bodies.get_mut(*rigid_body_handle) {
                rigid_body.set_next_kinematic_translation(next_translation);
            }
        }
    }

    /// Leaves every rigid body not entirely inside a region behind as static cells at rest in the
    /// active cellular tiles, without waiting for the GPU
    pub fn release_rigid_bodies(
        &mut self,
        accelerator: &Accelerator,
        active_region: CellularTileRegion,
        ring_offset: (u32, u32),
        region: CellularTileRegion,
    ) {
        let (region_x, region_y): (i32, i32) = self.rigid_body_region_origin(region);
        let region_width: f32 = (region.width() as i64 * CELLULAR_TILE_SIZE) as f32;
        let region_height: f32 = (region.height() as i64 * CELLULAR_TILE_SIZE) as f32;
        // a body is inside while every corner of its tiles is
        let released_rigid_bodies: Vec<u64> = self.collision_rigid_bodies.iter()
            .filter(|(_, collision_rigid_body)| {
                let Some((position, rotation, ..)) = collision_rigid_body.rasterization
                else { return false; };
                collision_rigid_body.cellular_tiles.keys().any(|(tile_x, tile_y)| {
                    [(0, 0), (8, 0), (0, 8), (8, 8)].into_iter().any(|(corner_x, corner_y)| {
                        let local_x: f32 = (tile_x * 8 + corner_x) as f32;
                        let local_y: f32 = (tile_y * 8 + corner_y) as f32;
                        let x: f32 = position[0] + rotation[0] * local_x - rotation[1] * local_y
                            - region_x as f32;
                        let y: f32 = position[1] + rotation[1] * local_x + rotation[0] * local_y
                            - region_y as f32;
                        x < 0.0 || y < 0.0 || x > region_width || y > region_height
                    })
                })
            })
            .map(|(rigid_body, _)| *rigid_body)
            .collect();
        if released_rigid_bodies.is_empty() { return; }
        let mut records: Vec<(u32, [f32; 14])> = Vec::new();
        let mut returned_cellular_tiles: Vec<u32> = Vec::new();
        let mut released_indices: Vec<u32> = Vec::new();
        for rigid_body in released_rigid_bodies {
            let Some(collision_rigid_body) = self.collision_rigid_bodies.remove(&rigid_body)
            else { continue; };
            if let Some(record) = self.rigid_body_record(&collision_rigid_body) {
                records.push((collision_rigid_body.index, record));
            }
            returned_cellular_tiles.extend(
                collision_rigid_body.cellular_tiles.values().map(|(_, _, _, _, index, _)| *index),
            );
            released_indices.push(collision_rigid_body.index);
            self.rigid_bodies.remove(
                collision_rigid_body.rigid_body_handle,
                &mut self.island_manager,
                &mut self.colliders,
                &mut self.impulse_joints,
                &mut self.multibody_joints,
                true,
            );
        }
        let region_origin: (i32, i32) = self.rigid_body_region_origin(active_region);
        self.rigid_body_pass.write_rigid_bodies(accelerator, &records);
        self.rigid_body_pass.rasterize_rigid_bodies(
            accelerator,
            active_region,
            ring_offset,
            region_origin,
            &[],
            &returned_cellular_tiles,
            &[],
            &[],
        );
        // freed only after the queued release, so later writes never reach it
        for cellular_tile in returned_cellular_tiles {
            self.cellular_tile_rigid_bodies.remove(&cellular_tile);
            self.free_cellular_tile_indices.push(cellular_tile);
        }
        self.free_rigid_body_indices.extend(released_indices);
        // the cells left behind are static cells of the active tiles
        self.is_cellular_tile_read_requested = true;
    }

    /// Returns the record index and identifier of every rigid body
    pub fn rigid_bodies(&self) -> Vec<(u32, u64)> {
        self.collision_rigid_bodies.iter()
            .map(|(rigid_body, collision_rigid_body)| (collision_rigid_body.index, *rigid_body))
            .collect()
    }

    /// Returns physical tiles around every rigid body's previous and current occupied bounds
    pub fn rigid_body_fluid_tiles(
        &self,
        region: CellularTileRegion,
        ring_offset: (u32, u32),
    ) -> Vec<u32> {
        let region_width: i32 = region.width() as i32;
        let region_height: i32 = region.height() as i32;
        let cell_size: f32 = CELLULAR_TILE_SIZE as f32;
        let (region_x, region_y): (i32, i32) = self.rigid_body_region_origin(region);
        let mut physical_tiles: HashSet<u32> = HashSet::new();
        for collision_rigid_body in self.collision_rigid_bodies.values() {
            let Some(rigid_body) = self.rigid_bodies.get(collision_rigid_body.rigid_body_handle)
            else { continue; };
            let current_position: [f32; 2] = [
                rigid_body.translation().x * cell_size,
                rigid_body.translation().y * cell_size,
            ];
            let current_rotation: [f32; 2] = [
                rigid_body.rotation().cos(),
                rigid_body.rotation().sin(),
            ];
            let mut transforms: Vec<([f32; 2], [f32; 2])> =
                vec![(current_position, current_rotation)];
            if let Some((position, rotation, ..)) = collision_rigid_body.rasterization {
                transforms.push((position, rotation));
            }
            for ((tile_x, tile_y), (cells, ..)) in &collision_rigid_body.cellular_tiles {
                if *cells == 0 { continue; }
                for (position, rotation) in &transforms {
                    let mut minimum: [f32; 2] = [f32::INFINITY; 2];
                    let mut maximum: [f32; 2] = [f32::NEG_INFINITY; 2];
                    for (corner_x, corner_y) in [(0.0, 0.0), (8.0, 0.0), (0.0, 8.0), (8.0, 8.0)] {
                        let local_x: f32 = *tile_x as f32 * 8.0 + corner_x;
                        let local_y: f32 = *tile_y as f32 * 8.0 + corner_y;
                        let x: f32 = position[0] + rotation[0] * local_x
                            - rotation[1] * local_y - region_x as f32;
                        let y: f32 = position[1] + rotation[1] * local_x
                            + rotation[0] * local_y - region_y as f32;
                        minimum[0] = minimum[0].min(x);
                        minimum[1] = minimum[1].min(y);
                        maximum[0] = maximum[0].max(x);
                        maximum[1] = maximum[1].max(y);
                    }
                    let minimum_tile_x: i32 = (minimum[0] / cell_size).floor() as i32 - 1;
                    let minimum_tile_y: i32 = (minimum[1] / cell_size).floor() as i32 - 1;
                    let maximum_tile_x: i32 = (maximum[0] / cell_size).floor() as i32 + 1;
                    let maximum_tile_y: i32 = (maximum[1] / cell_size).floor() as i32 + 1;
                    for tile_y in minimum_tile_y.max(0)..=maximum_tile_y.min(region_height - 1) {
                        for tile_x in minimum_tile_x.max(0)..=maximum_tile_x.min(region_width - 1) {
                            let physical_x: u32 =
                                (tile_x as u32 + ring_offset.0) % region_width as u32;
                            let physical_y: u32 =
                                (tile_y as u32 + ring_offset.1) % region_height as u32;
                            physical_tiles.insert(physical_y * region_width as u32 + physical_x);
                        }
                    }
                }
            }
        }
        let mut physical_tiles: Vec<u32> = physical_tiles.into_iter().collect();
        physical_tiles.sort_unstable();
        physical_tiles
    }

    /// Returns the cellular tile whose bottom-left corner is the origin of every Rapier position
    pub fn origin(&self) -> CellularTileCoordinates { self.origin }

    /// Returns the edges of every collider as scene-position line segments
    pub fn collider_outlines(&self) -> Vec<(ScenePosition, ScenePosition)> {
        use rapier2d::{math::Vector, parry::shape::AxisMask};

        let origin_position: ScenePosition = ScenePosition {
            tile_x: self.origin.x,
            tile_y: self.origin.y,
            offset_x: 0,
            offset_y: 0,
        };
        // collider-local points move by the collider pose, then from the collision origin
        let outline = |pose: &rapier2d::math::Pose, start: Vector, end: Vector| {
            let start: Vector = pose.translation + pose.rotation * start;
            let end: Vector = pose.translation + pose.rotation * end;
            (
                origin_position.translate(f64::from(start.x), f64::from(start.y)),
                origin_position.translate(f64::from(end.x), f64::from(end.y)),
            )
        };
        let mut outlines: Vec<(ScenePosition, ScenePosition)> = Vec::new();
        for (_, collider) in self.colliders.iter() {
            let pose: &rapier2d::math::Pose = collider.position();
            let mut points: Vec<Vector> = Vec::new();
            if let Some(voxels) = collider.shape().as_voxels() {
                let half_size: Vector = voxels.voxel_size() / 2.0;
                for voxel in voxels.voxels() {
                    if voxel.state.is_empty() { continue; }
                    let faces: AxisMask = voxel.state.free_faces();
                    let center: Vector = voxel.center;
                    let edges: [(AxisMask, [Vector; 2]); 4] = [
                        (AxisMask::X_NEG, [
                            center + Vector::new(-half_size.x, -half_size.y),
                            center + Vector::new(-half_size.x, half_size.y),
                        ]),
                        (AxisMask::X_POS, [
                            center + Vector::new(half_size.x, -half_size.y),
                            center + Vector::new(half_size.x, half_size.y),
                        ]),
                        (AxisMask::Y_NEG, [
                            center + Vector::new(-half_size.x, -half_size.y),
                            center + Vector::new(half_size.x, -half_size.y),
                        ]),
                        (AxisMask::Y_POS, [
                            center + Vector::new(-half_size.x, half_size.y),
                            center + Vector::new(half_size.x, half_size.y),
                        ]),
                    ];
                    for (face, [start, end]) in edges {
                        if faces.contains(face) { outlines.push(outline(pose, start, end)); }
                    }
                }
                continue;
            }
            if let Some(ball) = collider.shape().as_ball() {
                for index in 0..48 {
                    let angle: f32 = index as f32 * std::f32::consts::TAU / 48.0;
                    points.push(Vector::new(
                        angle.cos() * ball.radius,
                        angle.sin() * ball.radius,
                    ));
                }
            } else if let Some(cuboid) = collider.shape().as_cuboid() {
                let half_extents: Vector = cuboid.half_extents;
                points.extend([
                    Vector::new(-half_extents.x, -half_extents.y),
                    Vector::new(half_extents.x, -half_extents.y),
                    Vector::new(half_extents.x, half_extents.y),
                    Vector::new(-half_extents.x, half_extents.y),
                ]);
            } else if let Some(capsule) = collider.shape().as_capsule() {
                let start: Vector = capsule.segment.a;
                let end: Vector = capsule.segment.b;
                let angle: f32 = (end.y - start.y).atan2(end.x - start.x);
                for index in 0..=12 {
                    let arc: f32 = angle - std::f32::consts::FRAC_PI_2
                        + index as f32 * std::f32::consts::PI / 12.0;
                    points.push(end + Vector::new(
                        arc.cos() * capsule.radius,
                        arc.sin() * capsule.radius,
                    ));
                }
                for index in 0..=12 {
                    let arc: f32 = angle + std::f32::consts::FRAC_PI_2
                        + index as f32 * std::f32::consts::PI / 12.0;
                    points.push(start + Vector::new(
                        arc.cos() * capsule.radius,
                        arc.sin() * capsule.radius,
                    ));
                }
            } else if let Some(polygon) = collider.shape().as_convex_polygon() {
                points.extend(polygon.points().iter().copied());
            }
            for index in 0..points.len() {
                outlines.push(outline(pose, points[index], points[(index + 1) % points.len()]));
            }
        }
        outlines
    }

    /// Returns the GPU rasterization of rigid body cells
    pub fn rigid_body_pass(&self) -> &CollisionRigidBodyPass { &self.rigid_body_pass }

    /// Returns actors whose prior collision step selected swimming movement
    pub(crate) fn swimming_actors(&self) -> HashSet<Actor> {
        self.actors.iter()
            .filter_map(|(actor, (_, _, _, _, _, pawn))| {
                pawn.as_ref().and_then(|(_, _, _, swimming)| (*swimming).then_some(*actor))
            })
            .collect()
    }

    /// Detaches every rigid body cell an edit replaced from its body, waiting for the GPU, then
    /// splits the bodies it divides
    fn simulate_collisions_read_changed_cells(
        &mut self,
        accelerator: &Accelerator,
        active_region: CellularTileRegion,
        ring_offset: (u32, u32),
        material_table: &MaterialTable,
    ) -> Result<(), SceneError> {
        let mut records: Vec<(u32, [f32; 14])> = Vec::new();
        let mut cellular_tiles: Vec<u32> = Vec::new();
        for collision_rigid_body in self.collision_rigid_bodies.values() {
            let Some(record) = self.rigid_body_record(collision_rigid_body) else { continue; };
            records.push((collision_rigid_body.index, record));
            cellular_tiles.extend(
                collision_rigid_body.cellular_tiles.values().map(|(_, _, _, _, index, _)| *index),
            );
        }
        if cellular_tiles.is_empty() { return Ok(()); }
        self.rigid_body_pass.write_rigid_bodies(accelerator, &records);
        let region_origin: (i32, i32) = self.rigid_body_region_origin(active_region);
        let (changed_cells, is_incomplete): (Vec<u32>, bool) = self.rigid_body_pass
            .read_changed_cells(
                accelerator,
                active_region,
                ring_offset,
                region_origin,
                &cellular_tiles,
            )?;
        // cells beyond a full report stay attached until the next read finds them
        if is_incomplete { self.is_cellular_tile_read_requested = true; }
        self.simulate_collisions_remove_rigid_body_cells(
            accelerator,
            &changed_cells,
            material_table,
        );
        Ok(())
    }

    /// Removes listed cells from their rigid bodies and splits every body they divide
    fn simulate_collisions_remove_rigid_body_cells(
        &mut self,
        accelerator: &Accelerator,
        changed_cells: &[u32],
        material_table: &MaterialTable,
    ) {
        let mut removed_cells: HashMap<u64, Vec<(i32, i32)>> = HashMap::new();
        for changed_cell in changed_cells {
            let Some((rigid_body, (tile_x, tile_y))) =
                self.cellular_tile_rigid_bodies.get(&(changed_cell / 64)).copied()
            else { continue; };
            let Some((cells, ..)) = self.collision_rigid_bodies.get_mut(&rigid_body)
                .and_then(|collision_rigid_body| {
                    collision_rigid_body.cellular_tiles.get_mut(&(tile_x, tile_y))
                })
            else { continue; };
            let cell: i32 = (*changed_cell % 64) as i32;
            *cells &= !(1 << cell);
            removed_cells.entry(rigid_body).or_default()
                .push((tile_x * 8 + cell % 8, tile_y * 8 + cell / 8));
        }
        for (rigid_body, removed_cells) in removed_cells {
            self.simulate_collisions_split_rigid_body(
                accelerator,
                rigid_body,
                &removed_cells,
                material_table,
            );
        }
    }

    /// Turns every group of static cells disconnected from the rest that changed cells reach into
    /// a rigid body, or into debris when too small, unless it touches the active region's edge or
    /// is too large, requesting their materials for the next collision tick
    fn simulate_collisions_detach_rigid_bodies(
        &mut self,
        active_region: CellularTileRegion,
        ring_offset: (u32, u32),
        changed_cells: &[rapier2d::parry::math::IVector],
    ) {
        let Some((region, cellular_tiles)) = self.cellular_tiles.as_ref() else { return; };
        if *region != active_region || self.ring_offset != ring_offset { return; }
        let region_tile_width: i64 = region.width() as i64;
        let region_width: i64 = region_tile_width * CELLULAR_TILE_SIZE;
        let region_height: i64 = region.height() as i64 * CELLULAR_TILE_SIZE;
        let (region_x, region_y): (i32, i32) = self.rigid_body_region_origin(*region);
        let is_static = |x: i64, y: i64| -> bool {
            x >= 0 && y >= 0 && x < region_width && y < region_height
                && cellular_tiles[((y / 8) * region_tile_width + x / 8) as usize].as_u64()
                    >> ((y % 8) * 8 + x % 8) & 1 == 1
        };
        // flood each group once, and a group reaching an earlier one is attached through it
        let mut floods: HashMap<(i64, i64), usize> = HashMap::new();
        let mut groups: Vec<Vec<(i64, i64)>> = Vec::new();
        for changed_cell in changed_cells {
            let x: i64 = i64::from(changed_cell.x) - i64::from(region_x);
            let y: i64 = i64::from(changed_cell.y) - i64::from(region_y);
            for start in [(x, y), (x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)] {
                if !is_static(start.0, start.1) || floods.contains_key(&start) { continue; }
                let flood: usize = floods.len();
                floods.insert(start, flood);
                let mut group: Vec<(i64, i64)> = Vec::new();
                let mut pending_cells: Vec<(i64, i64)> = vec![start];
                let mut is_attached: bool = false;
                'flood: while let Some((cell_x, cell_y)) = pending_cells.pop() {
                    group.push((cell_x, cell_y));
                    // cells beyond the region may hold the group in place
                    if cell_x == 0 || cell_y == 0
                        || cell_x == region_width - 1 || cell_y == region_height - 1
                        || group.len() > COLLISION_RIGID_BODY_CELL_COUNT_MAXIMUM
                    {
                        is_attached = true;
                        break;
                    }
                    for neighbor in [
                        (cell_x + 1, cell_y),
                        (cell_x - 1, cell_y),
                        (cell_x, cell_y + 1),
                        (cell_x, cell_y - 1),
                    ] {
                        if !is_static(neighbor.0, neighbor.1) { continue; }
                        match floods.get(&neighbor) {
                            Some(neighbor_flood) if *neighbor_flood != flood => {
                                is_attached = true;
                                break 'flood;
                            }
                            Some(_) => {}
                            None => {
                                floods.insert(neighbor, flood);
                                pending_cells.push(neighbor);
                            }
                        }
                    }
                }
                if !is_attached { groups.push(group); }
            }
        }
        self.detached_cell_groups.extend(groups.into_iter().map(|group| {
            group.into_iter().map(|(x, y)| (x as u32, y as u32)).collect()
        }));
    }

    /// Requests all detach group reads together, without waiting for the GPU
    fn simulate_collisions_request_detached_cells(
        &mut self,
        accelerator: &Accelerator,
        active_region: CellularTileRegion,
        ring_offset: (u32, u32),
    ) {
        let groups: Vec<Vec<(u32, u32)>> = std::mem::take(&mut self.detached_cell_groups);
        let remaining_groups: Vec<Vec<(u32, u32)>> = self.rigid_body_pass
            .request_cell_reads(accelerator, active_region, ring_offset, groups);
        self.detached_cell_groups.extend(remaining_groups);
    }

    /// Reads detach group cells from the prior tick and verifies they are still static
    fn simulate_collisions_read_detached_cells(
        &mut self,
        accelerator: &Accelerator,
        active_region: CellularTileRegion,
        ring_offset: (u32, u32),
        material_table: &MaterialTable,
    ) -> Result<(), SceneError> {
        for (read_region, read_ring_offset, groups) in
            self.rigid_body_pass.read_cell_reads(accelerator)?
        {
            if read_region != active_region || read_ring_offset != ring_offset { continue; }
            let Some((region, cellular_tiles)) = self.cellular_tiles.as_ref() else { continue; };
            if *region != active_region { continue; }
            let width: usize = region.width();
            let mut verified_groups: Vec<
                Vec<((u32, u32), (MaterialIdentifier, u32, [u32; 2]))>,
            > = Vec::new();
            for group in groups {
                let is_current: bool = group.iter().all(|((x, y), _)| {
                    cellular_tiles[(*y as usize / 8) * width + *x as usize / 8].as_u64()
                        >> ((*y % 8) * 8 + *x % 8) & 1 == 1
                });
                if is_current { verified_groups.push(group); }
            }
            self.simulate_collisions_apply_detached_cells(
                accelerator,
                active_region,
                verified_groups,
                material_table,
            );
        }
        Ok(())
    }

    /// Creates rigid bodies or debris from verified detach groups and clears their static voxels
    fn simulate_collisions_apply_detached_cells(
        &mut self,
        accelerator: &Accelerator,
        active_region: CellularTileRegion,
        groups: Vec<Vec<((u32, u32), (MaterialIdentifier, u32, [u32; 2]))>>,
        material_table: &MaterialTable,
    ) {
        let region_tile_width: i64 = active_region.width() as i64;
        let (region_x, region_y): (i32, i32) = self.rigid_body_region_origin(active_region);
        for group in groups {
            let group_cells: Vec<((i64, i64), (MaterialIdentifier, u32, [u32; 2]))> =
                group.into_iter().map(|(cell, read)| {
                    ((i64::from(cell.0), i64::from(cell.1)), read)
                }).collect();
            let anchor_x: i64 = group_cells.iter().map(|((x, _), _)| *x).min().unwrap_or(0);
            let anchor_y: i64 = group_cells.iter().map(|((_, y), _)| *y).min().unwrap_or(0);
            let is_debris: bool = group_cells.len() < COLLISION_RIGID_BODY_CELL_COUNT_MINIMUM;
            if !is_debris {
                let mut body_cellular_tiles: BTreeMap<
                    (i32, i32),
                    (u64, [MaterialIdentifier; 64], [u32; 64], [[u32; 2]; 64], u32, u64),
                > = BTreeMap::new();
                for ((x, y), (material_identifier, color, state)) in &group_cells {
                    let local_x: i32 = (x - anchor_x) as i32;
                    let local_y: i32 = (y - anchor_y) as i32;
                    let (cells, material_identifiers, colors, states, _, _) = body_cellular_tiles
                        .entry((local_x.div_euclid(8), local_y.div_euclid(8)))
                        .or_insert((
                            0,
                            [MaterialIdentifier::EMPTY; 64],
                            [0; 64],
                            [[0; 2]; 64],
                            0,
                            0,
                        ));
                    let cell: usize = (local_y.rem_euclid(8) * 8 + local_x.rem_euclid(8)) as usize;
                    *cells |= 1 << cell;
                    material_identifiers[cell] = *material_identifier;
                    colors[cell] = *color;
                    states[cell] = *state;
                }
                let translation: rapier2d::math::Vector = rapier2d::math::Vector::new(
                    (i64::from(region_x) + anchor_x) as f32 / CELLULAR_TILE_SIZE as f32,
                    (i64::from(region_y) + anchor_y) as f32 / CELLULAR_TILE_SIZE as f32,
                );
                // groups beyond the rigid body capacity stay where they are
                if self.simulate_collisions_insert_rigid_body(
                    accelerator,
                    body_cellular_tiles,
                    translation,
                    0.0,
                    (rapier2d::math::Vector::ZERO, 0.0, translation),
                    material_table,
                ).is_none() { continue; }
            }
            // the cells leave the static voxels now so the new body never starts inside them
            for ((x, y), (material_identifier, _, _)) in group_cells {
                let material: u32 = if is_debris {
                    material_table.debris(material_identifier)
                        .unwrap_or(MaterialIdentifier::EMPTY)
                        .as_u32()
                } else { 0 };
                self.rigid_body_cell_writes
                    .push([x as u32, y as u32, material, material_identifier.as_u32()]);
                if let Some((_, cellular_tiles)) = self.cellular_tiles.as_mut() {
                    let tile_index: usize = (y / 8 * region_tile_width + x / 8) as usize;
                    cellular_tiles[tile_index] = CollisionCellularTile::from_u64(
                        cellular_tiles[tile_index].as_u64() & !(1 << ((y % 8) * 8 + x % 8)),
                    );
                }
                if let Some(voxels) = self.cellular_tile_collider
                    .and_then(|handle| self.colliders.get_mut(handle))
                    .and_then(|collider| collider.shape_mut().as_voxels_mut())
                {
                    voxels.set_voxel(
                        rapier2d::parry::math::IVector::new(
                            (i64::from(region_x) + x) as i32,
                            (i64::from(region_y) + y) as i32,
                        ),
                        false,
                    );
                    self.solid_cell_count -= 1;
                }
            }
        }
    }

    /// Creates a rigid body of cells placed at a translation in tiles and rotation, moving with
    /// the velocity in tiles per second and angular velocity of a body around a center in tiles,
    /// and returns its identifier, or `None` beyond the rigid body capacity
    fn simulate_collisions_insert_rigid_body(
        &mut self,
        accelerator: &Accelerator,
        mut cellular_tiles: BTreeMap<
            (i32, i32),
            (u64, [MaterialIdentifier; 64], [u32; 64], [[u32; 2]; 64], u32, u64),
        >,
        translation: rapier2d::math::Vector,
        rotation: f32,
        (velocity, angular_velocity, velocity_center): (
            rapier2d::math::Vector,
            f32,
            rapier2d::math::Vector,
        ),
        material_table: &MaterialTable,
    ) -> Option<u64> {
        if self.free_cellular_tile_indices.len() < cellular_tiles.len() { return None; }
        let index: u32 = self.free_rigid_body_indices.pop()?;
        for (_, _, _, _, cellular_tile, rasterized_cells) in cellular_tiles.values_mut() {
            *cellular_tile = self.free_cellular_tile_indices.pop()?;
            *rasterized_cells = 0;
        }
        let mut collision_rigid_body: CollisionRigidBody = CollisionRigidBody {
            index,
            cellular_tiles,
            rigid_body_handle: rapier2d::dynamics::RigidBodyHandle::invalid(),
            collider_handle: rapier2d::geometry::ColliderHandle::invalid(),
            rasterization: None,
        };
        let (mass, center, inertia, friction, restitution): (f32, [f32; 2], f32, f32, f32) =
            collision_rigid_body.mass_properties(material_table);
        if mass <= 0.0 {
            self.free_rigid_body_indices.push(index);
            self.free_cellular_tile_indices.extend(
                collision_rigid_body.cellular_tiles.values().map(|(_, _, _, _, index, _)| *index),
            );
            return None;
        }
        // the body's center moves with the velocity of the body it came from at that center
        let center_offset: rapier2d::math::Vector = rapier2d::math::Vector::new(
            rotation.cos() * center[0] - rotation.sin() * center[1],
            rotation.sin() * center[0] + rotation.cos() * center[1],
        );
        let arm: rapier2d::math::Vector = translation + center_offset - velocity_center;
        let rigid_body: rapier2d::dynamics::RigidBody =
            rapier2d::dynamics::RigidBodyBuilder::dynamic()
                .translation(translation)
                .rotation(rotation)
                .linvel(velocity + rapier2d::math::Vector::new(-arm.y, arm.x) * angular_velocity)
                .angvel(angular_velocity)
                .soft_ccd_prediction(COLLISION_RIGID_BODY_CCD_PREDICTION)
                .additional_solver_iterations(COLLISION_RIGID_BODY_SOLVER_ITERATION_COUNT)
                .build();
        collision_rigid_body.rigid_body_handle = self.rigid_bodies.insert(rigid_body);
        // filled one voxel at a time because Parry 0.30's bulk-built chunk hierarchy can panic
        // when a chunk is later emptied
        let mut voxels: rapier2d::parry::shape::Voxels = rapier2d::parry::shape::Voxels::new(
            rapier2d::math::Vector::splat(1.0 / CELLULAR_TILE_SIZE as f32),
            &[],
        );
        for ((tile_x, tile_y), (cells, ..)) in &collision_rigid_body.cellular_tiles {
            let mut remaining_cells: u64 = *cells;
            while remaining_cells != 0 {
                let cell: i32 = remaining_cells.trailing_zeros() as i32;
                remaining_cells &= remaining_cells - 1;
                let key: rapier2d::parry::math::IVector = rapier2d::parry::math::IVector::new(
                    tile_x * 8 + cell % 8,
                    tile_y * 8 + cell / 8,
                );
                voxels.set_voxel(key, true);
            }
        }
        let collider: rapier2d::geometry::Collider = rapier2d::geometry::ColliderBuilder::new(
            rapier2d::geometry::SharedShape::new(voxels),
        )
            .mass_properties(rapier2d::parry::mass_properties::MassProperties::new(
                rapier2d::math::Vector::new(center[0], center[1]),
                mass,
                inertia,
            ))
            .friction(friction)
            .restitution(restitution)
            .collision_groups(rapier2d::geometry::InteractionGroups::new(
                rapier2d::geometry::Group::from_bits_retain(COLLISION_GROUP_RIGID_BODY),
                rapier2d::geometry::Group::ALL,
                rapier2d::geometry::InteractionTestMode::And,
            ))
            .build();
        collision_rigid_body.collider_handle = self.colliders.insert_with_parent(
            collider,
            collision_rigid_body.rigid_body_handle,
            &mut self.rigid_bodies,
        );
        let rigid_body: u64 = self.next_rigid_body_identifier;
        self.next_rigid_body_identifier += 1;
        for ((tile_x, tile_y), (cells, material_identifiers, colors, states, cellular_tile, _)) in
            &collision_rigid_body.cellular_tiles
        {
            self.rigid_body_pass.write_cellular_tile(
                accelerator,
                *cellular_tile,
                index,
                (*tile_x, *tile_y),
                *cells,
                0,
                material_identifiers,
                colors,
                states,
            );
            self.cellular_tile_rigid_bodies
                .insert(*cellular_tile, (rigid_body, (*tile_x, *tile_y)));
        }
        self.collision_rigid_bodies.insert(rigid_body, collision_rigid_body);
        Some(rigid_body)
    }

    /// Splits a rigid body that lost cells into its groups of side-connected cells: the largest
    /// keeps the body, other large enough groups become rigid bodies moving as they did, and the
    /// rest become debris moving as they did, all changing where the body was last rasterized
    fn simulate_collisions_split_rigid_body(
        &mut self,
        accelerator: &Accelerator,
        rigid_body: u64,
        removed_cells: &[(i32, i32)],
        material_table: &MaterialTable,
    ) {
        let Some(mut collision_rigid_body) = self.collision_rigid_bodies.remove(&rigid_body)
        else { return; };
        let Some((translation, rotation, velocity, angular_velocity, center)) = self.rigid_bodies
            .get(collision_rigid_body.rigid_body_handle)
            .map(|rigid_body| (
                rigid_body.translation(),
                rigid_body.rotation().angle(),
                rigid_body.linvel(),
                rigid_body.angvel(),
                rigid_body.center_of_mass(),
            ))
        else { return; };
        // every tile clears where the body was, turning debris cells into debris there
        self.cleared_cellular_tiles.extend(
            collision_rigid_body.cellular_tiles.values().map(|(_, _, _, _, index, _)| *index),
        );
        let mut detached_cells: Vec<(i32, i32)> = removed_cells.to_vec();
        let mut kept_cells: BTreeMap<(i32, i32), u64> = BTreeMap::new();
        for (group_index, group) in collision_rigid_body.connected_cells().into_iter().enumerate() {
            let cell_count: usize = group.values().map(|cells| cells.count_ones() as usize).sum();
            let is_large: bool = cell_count >= COLLISION_RIGID_BODY_CELL_COUNT_MINIMUM;
            if group_index == 0 && is_large {
                kept_cells = group;
                continue;
            }
            for ((tile_x, tile_y), cells) in &group {
                let mut remaining_cells: u64 = *cells;
                while remaining_cells != 0 {
                    let cell: i32 = remaining_cells.trailing_zeros() as i32;
                    remaining_cells &= remaining_cells - 1;
                    detached_cells.push((tile_x * 8 + cell % 8, tile_y * 8 + cell / 8));
                }
            }
            let group_cellular_tiles: BTreeMap<
                (i32, i32),
                (u64, [MaterialIdentifier; 64], [u32; 64], [[u32; 2]; 64], u32, u64),
            > = group.iter()
                .filter_map(|(coordinates, cells)| {
                    let (_, material_identifiers, colors, states, _, _) =
                        collision_rigid_body.cellular_tiles.get(coordinates)?;
                    Some((*coordinates, (*cells, *material_identifiers, *colors, *states, 0, 0)))
                })
                .collect();
            let inserted_rigid_body: Option<u64> = if is_large {
                self.simulate_collisions_insert_rigid_body(
                    accelerator,
                    group_cellular_tiles,
                    translation,
                    rotation,
                    (velocity, angular_velocity, center),
                    material_table,
                )
            } else { None };
            if let Some(inserted_rigid_body) = inserted_rigid_body {
                let mut cell_copies: Vec<(u32, u32)> = Vec::new();
                if let Some(new_body) = self.collision_rigid_bodies.get(&inserted_rigid_body) {
                    for (coordinates, cells) in &group {
                        let Some((_, _, _, _, old_tile, _)) =
                            collision_rigid_body.cellular_tiles.get(coordinates)
                        else { continue; };
                        let Some((_, _, _, _, new_tile, _)) =
                            new_body.cellular_tiles.get(coordinates)
                        else { continue; };
                        let mut remaining_cells: u64 = *cells;
                        while remaining_cells != 0 {
                            let cell: u32 = remaining_cells.trailing_zeros();
                            remaining_cells &= remaining_cells - 1;
                            cell_copies.push((old_tile * 64 + cell, new_tile * 64 + cell));
                        }
                    }
                }
                if let Some((region, _)) = self.cellular_tiles.as_ref() {
                    self.rigid_body_pass.copy_cell_states(
                        accelerator,
                        *region,
                        self.ring_offset,
                        &cell_copies,
                    );
                }
                continue;
            }
            // too small or beyond capacity, so the group's cells become their debris
            for (coordinates, cells) in group {
                let Some((_, material_identifiers, ..)) =
                    collision_rigid_body.cellular_tiles.get_mut(&coordinates)
                else { continue; };
                let mut remaining_cells: u64 = cells;
                while remaining_cells != 0 {
                    let cell: usize = remaining_cells.trailing_zeros() as usize;
                    remaining_cells &= remaining_cells - 1;
                    material_identifiers[cell] = material_table
                        .debris(material_identifiers[cell])
                        .unwrap_or(MaterialIdentifier::EMPTY);
                }
            }
        }
        // the body keeps only the largest group's cells, writing the debris its tiles hold
        let mut emptied_tiles: Vec<(i32, i32)> = Vec::new();
        for (
            coordinates,
            (cells, material_identifiers, colors, states, cellular_tile, rasterized_cells),
        ) in collision_rigid_body.cellular_tiles.iter_mut()
        {
            *cells = kept_cells.get(coordinates).copied().unwrap_or(0);
            self.rigid_body_pass.write_cellular_tile(
                accelerator,
                *cellular_tile,
                collision_rigid_body.index,
                *coordinates,
                *cells,
                *rasterized_cells,
                material_identifiers,
                colors,
                states,
            );
            if *cells == 0 { emptied_tiles.push(*coordinates); }
        }
        for coordinates in emptied_tiles {
            if let Some((_, _, _, _, cellular_tile, _)) =
                collision_rigid_body.cellular_tiles.remove(&coordinates)
            {
                self.removed_cellular_tiles.push(cellular_tile);
            }
        }
        let mass_properties: (f32, [f32; 2], f32, f32, f32) =
            collision_rigid_body.mass_properties(material_table);
        if collision_rigid_body.cellular_tiles.is_empty() || mass_properties.0 <= 0.0 {
            // the body is gone once this rasterization clears where it was
            if let Some(record) = self.rigid_body_record(&collision_rigid_body) {
                self.removed_rigid_bodies.push((collision_rigid_body.index, record));
            } else {
                self.free_rigid_body_indices.push(collision_rigid_body.index);
            }
            self.removed_cellular_tiles.extend(
                collision_rigid_body.cellular_tiles.values().map(|(_, _, _, _, index, _)| *index),
            );
            self.rigid_bodies.remove(
                collision_rigid_body.rigid_body_handle,
                &mut self.island_manager,
                &mut self.colliders,
                &mut self.impulse_joints,
                &mut self.multibody_joints,
                true,
            );
            return;
        }
        let (mass, center, inertia, friction, restitution): (f32, [f32; 2], f32, f32, f32) =
            mass_properties;
        if let Some(collider) = self.colliders.get_mut(collision_rigid_body.collider_handle) {
            if let Some(voxels) = collider.shape_mut().as_voxels_mut() {
                for (x, y) in detached_cells {
                    voxels.set_voxel(rapier2d::parry::math::IVector::new(x, y), false);
                }
            }
            collider.set_mass_properties(rapier2d::parry::mass_properties::MassProperties::new(
                rapier2d::math::Vector::new(center[0], center[1]),
                mass,
                inertia,
            ));
            collider.set_friction(friction);
            collider.set_restitution(restitution);
        }
        self.changed_rigid_bodies.insert(rigid_body);
        self.collision_rigid_bodies.insert(rigid_body, collision_rigid_body);
    }

    /// Rasterizes every rigid body that moved or changed where it is now, clearing where it was,
    /// after writing pending cells, without waiting for the GPU
    fn simulate_collisions_rasterize_rigid_bodies(
        &mut self,
        accelerator: &Accelerator,
        active_region: CellularTileRegion,
        ring_offset: (u32, u32),
    ) {
        let cell_size: f32 = CELLULAR_TILE_SIZE as f32;
        let time_step: f32 = self.integration_parameters.dt;
        let removed_rigid_bodies: Vec<(u32, [f32; 14])> =
            std::mem::take(&mut self.removed_rigid_bodies);
        let mut records: Vec<(u32, [f32; 14])> = removed_rigid_bodies.clone();
        let mut cleared_cellular_tiles: Vec<u32> = std::mem::take(&mut self.cleared_cellular_tiles);
        let mut rasterized_cellular_tiles: Vec<u32> = Vec::new();
        for (rigid_body, collision_rigid_body) in &mut self.collision_rigid_bodies {
            let Some(rapier_rigid_body) =
                self.rigid_bodies.get(collision_rigid_body.rigid_body_handle)
            else { continue; };
            let translation: rapier2d::math::Vector = rapier_rigid_body.translation() * cell_size;
            let rotation: &rapier2d::math::Rotation = rapier_rigid_body.rotation();
            let velocity: rapier2d::math::Vector =
                rapier_rigid_body.linvel() * cell_size * time_step;
            let rasterization: ([f32; 2], [f32; 2], [f32; 2], f32) = (
                [translation.x, translation.y],
                [rotation.cos(), rotation.sin()],
                [velocity.x, velocity.y],
                rapier_rigid_body.angvel() * time_step,
            );
            // resting bodies keep their cells, while moving ones clear their old cells first
            let is_changed: bool = self.changed_rigid_bodies.contains(rigid_body);
            if !is_changed && collision_rigid_body.rasterization == Some(rasterization) {
                continue;
            }
            if !is_changed && collision_rigid_body.rasterization.is_some() {
                cleared_cellular_tiles.extend(
                    collision_rigid_body.cellular_tiles.values()
                        .map(|(_, _, _, _, index, _)| *index),
                );
            }
            for (cells, _, _, _, cellular_tile, rasterized_cells) in
                collision_rigid_body.cellular_tiles.values_mut()
            {
                rasterized_cellular_tiles.push(*cellular_tile);
                *rasterized_cells = *cells;
            }
            let (previous_position, previous_rotation, ..) =
                collision_rigid_body.rasterization.unwrap_or(rasterization);
            let center: rapier2d::math::Vector = rapier_rigid_body.center_of_mass() * cell_size;
            records.push((collision_rigid_body.index, [
                rasterization.0[0],
                rasterization.0[1],
                rasterization.1[0],
                rasterization.1[1],
                previous_position[0],
                previous_position[1],
                previous_rotation[0],
                previous_rotation[1],
                rasterization.2[0],
                rasterization.2[1],
                center.x,
                center.y,
                rasterization.3,
                rapier_rigid_body.mass(),
            ]));
            collision_rigid_body.rasterization = Some(rasterization);
        }
        self.changed_rigid_bodies.clear();
        let cell_writes: Vec<[u32; 4]> = std::mem::take(&mut self.rigid_body_cell_writes);
        if !records.is_empty() || !cell_writes.is_empty() || !cleared_cellular_tiles.is_empty() {
            let region_origin: (i32, i32) = self.rigid_body_region_origin(active_region);
            self.rigid_body_pass.write_rigid_bodies(accelerator, &records);
            self.rigid_body_pass.rasterize_rigid_bodies(
                accelerator,
                active_region,
                ring_offset,
                region_origin,
                &cell_writes,
                &[],
                &cleared_cellular_tiles,
                &rasterized_cellular_tiles,
            );
        }
        // removed bodies and tiles are reused only after the queued rasterization read them
        for cellular_tile in std::mem::take(&mut self.removed_cellular_tiles) {
            self.cellular_tile_rigid_bodies.remove(&cellular_tile);
            self.free_cellular_tile_indices.push(cellular_tile);
        }
        self.free_rigid_body_indices
            .extend(removed_rigid_bodies.into_iter().map(|(index, _)| index));
    }

    /// Returns the record of a rigid body rasterized where it was last rasterized, or `None`
    /// before its first rasterization
    fn rigid_body_record(&self, collision_rigid_body: &CollisionRigidBody) -> Option<[f32; 14]> {
        let (position, rotation, velocity, angular_velocity) = collision_rigid_body.rasterization?;
        let rigid_body: &rapier2d::dynamics::RigidBody =
            self.rigid_bodies.get(collision_rigid_body.rigid_body_handle)?;
        let center: rapier2d::math::Vector =
            rigid_body.center_of_mass() * CELLULAR_TILE_SIZE as f32;
        Some([
            position[0],
            position[1],
            rotation[0],
            rotation[1],
            position[0],
            position[1],
            rotation[0],
            rotation[1],
            velocity[0],
            velocity[1],
            center.x,
            center.y,
            angular_velocity,
            rigid_body.mass(),
        ])
    }

    /// Returns a region's bottom-left cell relative to the origin
    fn rigid_body_region_origin(&self, region: CellularTileRegion) -> (i32, i32) {
        (
            ((region.bottom_left.x - self.origin.x) * CELLULAR_TILE_SIZE) as i32,
            ((region.bottom_left.y - self.origin.y) * CELLULAR_TILE_SIZE) as i32,
        )
    }

    /// Creates or updates an actor's body and replaces colliders whose components changed
    fn simulate_collisions_update_actor(
        &mut self,
        actor: Actor,
        position: ScenePosition,
        translation: rapier2d::math::Vector,
        collision: Option<ActorCollisionComponent>,
        contact: Option<ActorContactComponent>,
        is_pawn: bool,
    ) {
        // actors with only a contact are kinematic
        let rigid_body_type: rapier2d::dynamics::RigidBodyType =
            match collision.as_ref().map(|collision| collision.body) {
                Some(CollisionBody::Fixed) => rapier2d::dynamics::RigidBodyType::Fixed,
                Some(CollisionBody::Dynamic) => rapier2d::dynamics::RigidBodyType::Dynamic,
                Some(CollisionBody::Kinematic) | None =>
                    rapier2d::dynamics::RigidBodyType::KinematicPositionBased,
            };
        let rigid_body_handle: rapier2d::dynamics::RigidBodyHandle =
            match self.actors.entry(actor) {
                Entry::Occupied(entry) => entry.get().0,
                Entry::Vacant(entry) => {
                    // actor rotation is not simulated until actors have a rotation
                    let rigid_body: rapier2d::dynamics::RigidBody =
                        rapier2d::dynamics::RigidBodyBuilder::new(rigid_body_type)
                            .translation(translation)
                            .lock_rotations()
                            .build();
                    let rigid_body_handle: rapier2d::dynamics::RigidBodyHandle =
                        self.rigid_bodies.insert(rigid_body);
                    entry.insert((rigid_body_handle, None, None, position, translation, None)).0
                }
            };
        let Some((_, actor_collision, actor_contact, actor_position, actor_translation, pawn)) =
            self.actors.get_mut(&actor)
        else { return; };
        // pawns start at rest and ungrounded
        match (is_pawn, pawn.is_some()) {
            (true, false) => {
                *pawn = Some((rapier2d::math::Vector::ZERO, false, false, false));
            }
            (false, true) => *pawn = None,
            _ => {}
        }
        if let Some(rigid_body) = self.rigid_bodies.get_mut(rigid_body_handle) {
            if rigid_body.body_type() != rigid_body_type {
                rigid_body.set_body_type(rigid_body_type, true);
            }
            // move the body where gameplay placed the actor since the last step, at rest
            if *actor_position != position {
                *actor_position = position;
                *actor_translation = translation;
                if let Some((velocity, is_grounded, _, is_swimming)) = pawn {
                    *velocity = rapier2d::math::Vector::ZERO;
                    *is_grounded = false;
                    *is_swimming = false;
                }
                if rigid_body.is_kinematic() {
                    rigid_body.set_next_kinematic_translation(translation);
                } else {
                    rigid_body.set_translation(translation, true);
                }
            }
        }
        // replace each collider whose component changed
        let is_collision_changed: bool =
            actor_collision.as_ref().map(|(_, actor_collision)| actor_collision)
                != collision.as_ref();
        let is_contact_changed: bool =
            actor_contact.as_ref().map(|(_, actor_contact)| actor_contact) != contact.as_ref();
        let mut previous_collider_handles: Vec<rapier2d::geometry::ColliderHandle> = Vec::new();
        if is_collision_changed && let Some((collider_handle, _)) = actor_collision.take() {
            previous_collider_handles.push(collider_handle);
        }
        if is_contact_changed && let Some((collider_handle, _)) = actor_contact.take() {
            previous_collider_handles.push(collider_handle);
        }
        for collider_handle in previous_collider_handles {
            self.colliders.remove(
                collider_handle,
                &mut self.island_manager,
                &mut self.rigid_bodies,
                true,
            );
        }
        if is_collision_changed && let Some(collision) = collision {
            let collider_handle: rapier2d::geometry::ColliderHandle =
                self.simulate_collisions_insert_collider(
                    rigid_body_handle,
                    &collision.collider,
                    collision.memberships,
                    collision.filter,
                    false,
                    (collision.body == CollisionBody::Dynamic).then_some(collision.mass),
                );
            if let Some((_, actor_collision, _, _, _, _)) = self.actors.get_mut(&actor) {
                *actor_collision = Some((collider_handle, collision));
            }
        }
        if is_contact_changed && let Some(contact) = contact {
            let collider_handle: rapier2d::geometry::ColliderHandle =
                self.simulate_collisions_insert_collider(
                    rigid_body_handle,
                    &contact.collider,
                    contact.memberships,
                    contact.filter,
                    true,
                    None,
                );
            if let Some((_, _, actor_contact, _, _, _)) = self.actors.get_mut(&actor) {
                *actor_contact = Some((collider_handle, contact));
            }
        }
    }

    /// Attaches a validated collider to an actor body as a solid collider or a contact sensor
    fn simulate_collisions_insert_collider(
        &mut self,
        rigid_body_handle: rapier2d::dynamics::RigidBodyHandle,
        collider: &Collider,
        memberships: u32,
        filter: u32,
        is_sensor: bool,
        mass: Option<f32>,
    ) -> rapier2d::geometry::ColliderHandle {
        // contact sensors sense every body type, including fixed and kinematic ones
        let active_collision_types: rapier2d::geometry::ActiveCollisionTypes = if is_sensor {
            rapier2d::geometry::ActiveCollisionTypes::all()
        } else {
            rapier2d::geometry::ActiveCollisionTypes::default()
        };
        let mut builder: rapier2d::geometry::ColliderBuilder =
            rapier2d::geometry::ColliderBuilder::new(Self::rapier_shape(collider));
        if let Some(mass) = mass { builder = builder.mass(mass); }
        let rapier_collider: rapier2d::geometry::Collider = builder
                .position(rapier2d::math::Pose::new(
                    rapier2d::math::Vector::new(collider.offset_x, collider.offset_y),
                    collider.rotation,
                ))
                .collision_groups(rapier2d::geometry::InteractionGroups::new(
                    rapier2d::geometry::Group::from_bits_retain(memberships),
                    rapier2d::geometry::Group::from_bits_retain(
                        filter & !COLLISION_GROUP_CELLULAR_DYNAMIC,
                    ),
                    rapier2d::geometry::InteractionTestMode::And,
                ))
                .sensor(is_sensor)
                .active_collision_types(active_collision_types)
                .build();
        self.colliders.insert_with_parent(
            rapier_collider,
            rigid_body_handle,
            &mut self.rigid_bodies,
        )
    }

    /// Returns the bounds of a collider on an actor, rejecting invalid dimensions
    fn simulate_collisions_collider_bounds(
        actor: Actor,
        collider: &Collider,
        translation: rapier2d::math::Vector,
    ) -> Result<rapier2d::parry::bounding_volume::Aabb, SceneError> {
        let is_valid: bool = collider.offset_x.is_finite()
            && collider.offset_y.is_finite()
            && collider.rotation.is_finite()
            && match collider.shape {
                ColliderShape::Circle { radius } => radius.is_finite() && radius > 0.0,
                ColliderShape::Rectangle { width, height } =>
                    width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0,
                ColliderShape::Capsule { width, height } =>
                    width.is_finite() && height.is_finite() && width > 0.0 && height >= width,
            };
        if !is_valid { return Err(SceneError::InvalidCollider { actor }); }
        let pose: rapier2d::math::Pose = rapier2d::math::Pose::new(
            translation + rapier2d::math::Vector::new(collider.offset_x, collider.offset_y),
            collider.rotation,
        );
        Ok(Self::rapier_shape(collider).compute_aabb(&pose))
    }

    /// Returns the Rapier shape of a collider centered on its offset
    fn rapier_shape(collider: &Collider) -> rapier2d::geometry::SharedShape {
        match collider.shape {
            ColliderShape::Circle { radius } =>
                rapier2d::geometry::SharedShape::ball(radius),
            ColliderShape::Rectangle { width, height } =>
                rapier2d::geometry::SharedShape::cuboid(width / 2.0, height / 2.0),
            ColliderShape::Capsule { width, height } =>
                rapier2d::geometry::SharedShape::capsule_y((height - width) / 2.0, width / 2.0),
        }
    }

    /// Returns the Rapier translation of an exact scene position relative to the origin
    fn rapier_translation(
        origin: CellularTileCoordinates,
        position: ScenePosition,
    ) -> rapier2d::math::Vector {
        let origin_position: ScenePosition =
            ScenePosition { tile_x: origin.x, tile_y: origin.y, offset_x: 0, offset_y: 0 };
        let (x, y): (f64, f64) = position.relative_position(origin_position);
        rapier2d::math::Vector::new(x as f32, y as f32)
    }

}
