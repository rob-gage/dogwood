// Copyright Rob Gage 2026

use super::{
    scene_collider::SceneCollider,
    scene_collider_shape::SceneColliderShape,
    scene_collision_body::SceneCollisionBody,
    scene_collision_cellular_tile::SceneCollisionCellularTile,
    scene_collision_cellular_tile_pass::SceneCollisionCellularTilePass,
    SCENE_COLLISION_ORIGIN_DISTANCE_MAXIMUM,
};
use crate::{
    actor_components::{
        ActorCollisionComponent,
        ActorContactComponent,
        ActorIdentityComponent,
        ActorKinematicsPositionComponent,
        ActorPawnControlComponent,
        ActorPawnMovementComponent,
        ActorPersistenceComponent,
    },
    actors::{
        Actor,
        ActorError,
        ActorRegistry,
    },
    cellular_tiles::{
        CellularTileBuffers,
        CellularTileCoordinates,
        CellularTileRegion,
        CELLULAR_TILE_SIZE,
    },
    scenes::{
        SceneError,
        ScenePosition,
    },
};
use engine_host::acceleration::Accelerator;
use std::collections::{
    hash_map::Entry,
    HashMap,
    HashSet,
};

/// Rapier simulation of a scene's actors against the solid cells of its active cellular tiles
pub struct SceneCollision {
    /// Cellular tile whose bottom-left corner is the origin of every Rapier position
    origin: CellularTileCoordinates,
    /// Active region and occupancies held by the cellular tile collider, absent before a readback
    cellular_tiles: Option<(CellularTileRegion, Vec<SceneCollisionCellularTile>)>,
    /// Ring offset of the active region when its occupancies were read
    ring_offset: (u32, u32),
    /// Number of solid cells held by the cellular tile collider
    solid_cell_count: u64,
    /// Fixed voxel collider of every solid cell in the active region, absent while none are solid
    cellular_tile_collider: Option<rapier2d::geometry::ColliderHandle>,
    /// Body, colliders, position, translation, and pawn velocity, grounding, and held jump
    actors: HashMap<Actor, (
        rapier2d::dynamics::RigidBodyHandle,
        Option<(rapier2d::geometry::ColliderHandle, ActorCollisionComponent)>,
        Option<(rapier2d::geometry::ColliderHandle, ActorContactComponent)>,
        ScenePosition,
        rapier2d::math::Vector,
        Option<(rapier2d::math::Vector, bool, bool)>,
    )>,
    /// Character controller moving every pawn against the colliders it collides with
    pawn_movement_controller: rapier2d::control::KinematicCharacterController,
    /// GPU extraction of active cellular tile occupancies
    cellular_tile_pass: SceneCollisionCellularTilePass,
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

impl SceneCollision {

    /// Creates an empty simulation reading occupancies from the cellular tile buffers
    pub fn new(accelerator: &Accelerator, cellular_tile_buffers: &CellularTileBuffers) -> Self {
        Self {
            origin: CellularTileCoordinates { x: 0, y: 0 },
            cellular_tiles: None,
            ring_offset: (0, 0),
            solid_cell_count: 0,
            cellular_tile_collider: None,
            actors: HashMap::new(),
            // round pawns roll over steps lower than their radius, so taller ledges are walls
            pawn_movement_controller: rapier2d::control::KinematicCharacterController {
                offset: rapier2d::control::CharacterLength::Absolute(0.01),
                max_slope_climb_angle: 50.0_f32.to_radians(),
                min_slope_slide_angle: 50.0_f32.to_radians(),
                snap_to_ground: Some(rapier2d::control::CharacterLength::Absolute(0.2)),
                ..rapier2d::control::KinematicCharacterController::default()
            },
            cellular_tile_pass: SceneCollisionCellularTilePass::new(
                accelerator,
                cellular_tile_buffers,
            ),
            rigid_bodies: rapier2d::dynamics::RigidBodySet::new(),
            colliders: rapier2d::geometry::ColliderSet::new(),
            integration_parameters: rapier2d::dynamics::IntegrationParameters::default(),
            physics_pipeline: rapier2d::pipeline::PhysicsPipeline::new(),
            island_manager: rapier2d::dynamics::IslandManager::new(),
            broad_phase: rapier2d::geometry::BroadPhaseBvh::new(),
            narrow_phase: rapier2d::geometry::NarrowPhase::new(),
            impulse_joints: rapier2d::dynamics::ImpulseJointSet::new(),
            multibody_joints: rapier2d::dynamics::MultibodyJointSet::new(),
            ccd_solver: rapier2d::dynamics::CCDSolver::new(),
        }
    }

    /// Synchronizes the active cellular tiles and actors, then advances one fixed step
    pub fn simulate_collisions(
        &mut self,
        accelerator: &Accelerator,
        active_region: CellularTileRegion,
        ring_offset: (u32, u32),
        gravity: f32,
        actor_registry: &mut ActorRegistry,
    ) -> Result<(), SceneError> {
        // GPU cells only change when streaming moves the ring, so an unmoved ring needs no readback
        if self.cellular_tiles.as_ref().is_some_and(|(region, _)| *region == active_region)
            && self.ring_offset == ring_offset
        { return self.simulate_collisions_step_actors(active_region, gravity, actor_registry); }
        // read this tick's GPU occupancy before simulating against it
        let cellular_tiles: Vec<SceneCollisionCellularTile> = self.cellular_tile_pass
            .read_cellular_tiles(accelerator, active_region, ring_offset)?;
        // keep the origin near the active region so f32 positions keep sub-cell precision
        let center: CellularTileCoordinates = CellularTileCoordinates {
            x: active_region.bottom_left.x + active_region.width() as i64 / 2,
            y: active_region.bottom_left.y + active_region.height() as i64 / 2,
        };
        if self.cellular_tiles.is_none()
            || center.x.abs_diff(self.origin.x) > SCENE_COLLISION_ORIGIN_DISTANCE_MAXIMUM
            || center.y.abs_diff(self.origin.y) > SCENE_COLLISION_ORIGIN_DISTANCE_MAXIMUM
        { self.simulate_collisions_move_origin(center); }
        self.simulate_collisions_apply_cellular_tiles(active_region, cellular_tiles);
        self.ring_offset = ring_offset;
        self.simulate_collisions_step_actors(active_region, gravity, actor_registry)
    }

    /// Moves the origin, removing the cellular tile collider and re-placing every body
    fn simulate_collisions_move_origin(&mut self, origin: CellularTileCoordinates) {
        self.origin = origin;
        if let Some(cellular_tile_collider) = self.cellular_tile_collider.take() {
            self.colliders.remove(
                cellular_tile_collider,
                &mut self.island_manager,
                &mut self.rigid_bodies,
                false,
            );
        }
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

    /// Applies cells that changed, entered, or left the active region to the voxel collider
    fn simulate_collisions_apply_cellular_tiles(
        &mut self,
        active_region: CellularTileRegion,
        cellular_tiles: Vec<SceneCollisionCellularTile>,
    ) {
        let mut cell_changes: Vec<(rapier2d::parry::math::IVector, bool)> = Vec::new();
        let previous_cellular_tiles: Option<(
            CellularTileRegion,
            Vec<SceneCollisionCellularTile>,
        )> = self.cellular_tiles.take();
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
        self.cellular_tiles = Some((active_region, cellular_tiles));
        if cell_changes.is_empty() { return; }
        for (_, is_filled) in &cell_changes {
            if *is_filled { self.solid_cell_count += 1; } else { self.solid_cell_count -= 1; }
        }
        // edit the existing voxels in place, creating the collider once a cell becomes solid
        match self.cellular_tile_collider
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
                    ).build();
                self.cellular_tile_collider = Some(self.colliders.insert(collider));
            }
        }
        if self.solid_cell_count == 0 && let Some(cellular_tile_collider) =
            self.cellular_tile_collider.take()
        {
            self.colliders.remove(
                cellular_tile_collider,
                &mut self.island_manager,
                &mut self.rigid_bodies,
                false,
            );
        }
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
                        .is_some_and(|collision| collision.body == SceneCollisionBody::Kinematic)
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
        self.simulate_collisions_move_pawns(&pawns, gravity);
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

    /// Moves every pawn by its controlled velocity and gravity with the character controller
    fn simulate_collisions_move_pawns(
        &mut self,
        pawns: &[(Actor, ActorPawnMovementComponent, ActorPawnControlComponent)],
        gravity: f32,
    ) {
        let time_step: f32 = self.integration_parameters.dt;
        for (actor, pawn_movement, pawn_control) in pawns {
            let Some((rigid_body_handle, Some((collider_handle, _)), _, _, _, Some(pawn))) =
                self.actors.get_mut(actor)
            else { continue; };
            let (velocity, is_grounded, is_jump_held): &mut (rapier2d::math::Vector, bool, bool) =
                pawn;
            let (Some(rigid_body), Some(collider)) = (
                self.rigid_bodies.get(*rigid_body_handle),
                self.colliders.get(*collider_handle),
            ) else { continue; };
            // approach the controlled speed, jumping once per press while grounded
            let speed: f32 = if pawn_control.is_crouching {
                pawn_movement.crouching_speed
            } else {
                pawn_movement.speed
            };
            let target_velocity_x: f32 = pawn_control.movement.clamp(-1.0, 1.0) * speed;
            let velocity_change: f32 = pawn_movement.acceleration * time_step;
            velocity.x += (target_velocity_x - velocity.x).clamp(-velocity_change, velocity_change);
            if pawn_control.is_jumping && !*is_jump_held && *is_grounded {
                velocity.y = pawn_movement.jump_speed;
            }
            *is_jump_held = pawn_control.is_jumping;
            // grounded pawns rest on the ground instead of pressing into it
            if *is_grounded && velocity.y <= 0.0 {
                velocity.y = 0.0;
            } else {
                velocity.y -= gravity * time_step;
            }
            // cast from the body's next pose so gameplay placement this tick is respected
            let collider_pose: rapier2d::math::Pose = *rigid_body.next_position()
                * collider.position_wrt_parent().copied().unwrap_or_default();
            let desired_translation: rapier2d::math::Vector = *velocity * time_step;
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
            *is_grounded = movement.grounded;
            let next_translation: rapier2d::math::Vector =
                rigid_body.next_position().translation + movement.translation;
            if let Some(rigid_body) = self.rigid_bodies.get_mut(*rigid_body_handle) {
                rigid_body.set_next_kinematic_translation(next_translation);
            }
        }
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
                Some(SceneCollisionBody::Fixed) => rapier2d::dynamics::RigidBodyType::Fixed,
                Some(SceneCollisionBody::Dynamic) => rapier2d::dynamics::RigidBodyType::Dynamic,
                Some(SceneCollisionBody::Kinematic) | None =>
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
            (true, false) => *pawn = Some((rapier2d::math::Vector::ZERO, false, false)),
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
                if let Some((velocity, is_grounded, _)) = pawn {
                    *velocity = rapier2d::math::Vector::ZERO;
                    *is_grounded = false;
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
        collider: &SceneCollider,
        memberships: u32,
        filter: u32,
        is_sensor: bool,
    ) -> rapier2d::geometry::ColliderHandle {
        let shape: rapier2d::geometry::SharedShape = match collider.shape {
            SceneColliderShape::Circle { radius } =>
                rapier2d::geometry::SharedShape::ball(radius),
            SceneColliderShape::Rectangle { width, height } =>
                rapier2d::geometry::SharedShape::cuboid(width / 2.0, height / 2.0),
            SceneColliderShape::Capsule { width, height } =>
                rapier2d::geometry::SharedShape::capsule_y((height - width) / 2.0, width / 2.0),
        };
        // contact sensors sense every body type, including fixed and kinematic ones
        let active_collision_types: rapier2d::geometry::ActiveCollisionTypes = if is_sensor {
            rapier2d::geometry::ActiveCollisionTypes::all()
        } else {
            rapier2d::geometry::ActiveCollisionTypes::default()
        };
        let rapier_collider: rapier2d::geometry::Collider =
            rapier2d::geometry::ColliderBuilder::new(shape)
                .position(rapier2d::math::Pose::new(
                    rapier2d::math::Vector::new(collider.offset_x, collider.offset_y),
                    collider.rotation,
                ))
                .collision_groups(rapier2d::geometry::InteractionGroups::new(
                    rapier2d::geometry::Group::from_bits_retain(memberships),
                    rapier2d::geometry::Group::from_bits_retain(filter),
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
        collider: &SceneCollider,
        translation: rapier2d::math::Vector,
    ) -> Result<rapier2d::parry::bounding_volume::Aabb, SceneError> {
        let is_valid: bool = collider.offset_x.is_finite()
            && collider.offset_y.is_finite()
            && collider.rotation.is_finite()
            && match collider.shape {
                SceneColliderShape::Circle { radius } => radius.is_finite() && radius > 0.0,
                SceneColliderShape::Rectangle { width, height } =>
                    width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0,
                SceneColliderShape::Capsule { width, height } =>
                    width.is_finite() && height.is_finite() && width > 0.0 && height >= width,
            };
        if !is_valid { return Err(SceneError::InvalidCollider { actor }); }
        let pose: rapier2d::math::Pose = rapier2d::math::Pose::new(
            translation + rapier2d::math::Vector::new(collider.offset_x, collider.offset_y),
            collider.rotation,
        );
        Ok(match collider.shape {
            SceneColliderShape::Circle { radius } =>
                rapier2d::parry::shape::Shape::compute_aabb(
                    &rapier2d::parry::shape::Ball::new(radius),
                    &pose,
                ),
            SceneColliderShape::Rectangle { width, height } =>
                rapier2d::parry::shape::Shape::compute_aabb(
                    &rapier2d::parry::shape::Cuboid::new(
                        rapier2d::math::Vector::new(width / 2.0, height / 2.0),
                    ),
                    &pose,
                ),
            SceneColliderShape::Capsule { width, height } =>
                rapier2d::parry::shape::Shape::compute_aabb(
                    &rapier2d::parry::shape::Capsule::new_y((height - width) / 2.0, width / 2.0),
                    &pose,
                ),
        })
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
