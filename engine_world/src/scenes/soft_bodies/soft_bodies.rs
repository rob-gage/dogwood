// Copyright Rob Gage 2026

use super::{
    SoftBody,
    SoftBodyAnchor,
    SoftBodyDescriptor,
    SoftBodyEnvironment,
    SoftBodySegment,
    SoftBodyTerrain,
    COLLISION_GROUP_SOFT_BODY,
    COLLISION_GROUP_SOFT_BODY_SOLID,
    SOFT_BODY_CUT_GRACE,
    SOFT_BODY_REST_SPEED,
    SOFT_BODY_SEPARATION_STEP,
    SOFT_BODY_REST_STEP_COUNT,
    SOFT_BODY_SAMPLE_COUNT_MAXIMUM,
    SOFT_BODY_STEP_SECONDS,
    SOFT_BODY_TERRAIN_MARGIN,
};
use crate::{materials::{MaterialIdentifier, MaterialTable}, scenes::{
    cellular_tiles::{
        CellularTileCoordinates,
        CellularTileRegion,
        CELLULAR_TILE_SIZE,
    },
    collisions::COLLISION_GROUP_ACTOR,
    ScenePosition,
}};
use rapier2d::{
    dynamics::{
        ImpulseJointSet,
        IslandManager,
        MultibodyJointSet,
        RigidBodyHandle,
        RigidBodySet,
        SoftBodyBuilder,
        SoftBodyEdgeKind,
        SoftBodyHandle,
        SoftBodySet,
        SoftBodyTearEvent,
        SpringCoefficients,
    },
    geometry::{
        ColliderBuilder,
        ColliderSet,
        Group,
        InteractionGroups,
        InteractionTestMode,
    },
    math::Vector,
};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::Mutex,
};

/// Stable identifier of a soft body in one scene
#[derive(Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SceneSoftBody(pub(crate) u64);

/// Something that happened to a soft body during a simulation step
#[derive(Clone, Debug, PartialEq)]
pub enum SoftBodyEvent {
    /// An anchor let go because what held it is gone
    AnchorLost {
        /// The body
        body: SceneSoftBody,
        /// The released anchor
        anchor: SoftBodyAnchor,
    },
    /// A piece became too small to hold together: it is gone, and its cells became debris or nothing
    Decomposed {
        /// The body that was removed
        body: SceneSoftBody,
    },
    /// A body came apart: `pieces` lists each resulting body, the first keeping `body`'s
    /// identifier, with the particle index in `body` before the tear of each of its particles
    Torn {
        /// The body that tore
        body: SceneSoftBody,
        /// The resulting bodies with their particles' former indices
        pieces: Vec<(SceneSoftBody, Vec<u32>)>,
    },
}

/// A soft body's particles at one moment
#[derive(Clone, Debug, PartialEq)]
pub struct SoftBodyParticles {
    /// Scene position its positions are relative to
    pub origin: ScenePosition,
    /// Particle positions in tiles from `origin`
    pub positions: Vec<[f32; 2]>,
    /// Particle velocities in tiles per second
    pub velocities: Vec<[f32; 2]>,
    /// Outward normal of the solid cell each particle rests against, zero when free
    pub contacts: Vec<[f32; 2]>,
    /// Elastic triangles
    pub triangles: Vec<[u32; 3]>,
    /// Particle pairs of its outline, oriented with its inside to the left
    pub outline: Vec<[u32; 2]>,
    /// Particle masses
    pub masses: Vec<f32>,
}

/// Collects the tears Rapier reports during a step
#[derive(Default)]
pub(crate) struct SoftBodyTearEvents(pub(crate) Mutex<Vec<SoftBodyTearEvent>>);

impl rapier2d::pipeline::EventHandler for SoftBodyTearEvents {

    fn handle_collision_event(
        &self,
        _bodies: &RigidBodySet,
        _colliders: &ColliderSet,
        _event: rapier2d::geometry::CollisionEvent,
        _contact_pair: Option<&rapier2d::geometry::ContactPair>,
    ) {}

    fn handle_contact_force_event(
        &self,
        _dt: f32,
        _bodies: &RigidBodySet,
        _colliders: &ColliderSet,
        _contact_pair: &rapier2d::geometry::ContactPair,
        _total_force_magnitude: f32,
    ) {}

    fn handle_soft_body_tear_event(&self, _soft_bodies: &SoftBodySet, event: &SoftBodyTearEvent) {
        if let Ok(mut events) = self.0.lock() { events.push(event.clone()); }
    }

}

/// The parts of a Rapier world soft bodies are inserted into and removed from
pub(crate) struct SoftBodyRapier<'a> {
    /// Rigid bodies
    pub(crate) bodies: &'a mut RigidBodySet,
    /// Colliders
    pub(crate) colliders: &'a mut ColliderSet,
    /// Islands
    pub(crate) islands: &'a mut IslandManager,
    /// Impulse joints
    pub(crate) impulse_joints: &'a mut ImpulseJointSet,
    /// Multibody joints
    pub(crate) multibody_joints: &'a mut MultibodyJointSet,
}

/// The soft bodies of a scene: Rapier solves their constraints, contacts with actors, tearing, and
/// closed-body area, while this keeps them out of solid cells and couples them to the cell grid
pub(crate) struct SoftBodies {
    /// Rapier's soft bodies
    pub(crate) set: SoftBodySet,
    /// Live bodies, ordered so that their stepping is deterministic
    pub(crate) bodies: BTreeMap<SceneSoftBody, SoftBody>,
    /// Identifier of the next body, never reused
    next_identifier: u64,
    /// Tile at the origin of every particle position
    pub(crate) origin: CellularTileCoordinates,
    /// Events since they were last taken
    events: Vec<SoftBodyEvent>,
    /// Tick count, spreading sparse sampling of sleeping bodies across ticks
    tick: u32,
    /// Tags reported by particle environments
    pub(crate) environment_tags: Vec<String>,
    /// Whether the environment tags changed since the sampling pass read them
    pub(crate) is_environment_tags_changed: bool,
    /// Body, GPU record of the segment, and tile coordinates that hold each GPU tile
    pub(crate) tile_owners: HashMap<u32, (SceneSoftBody, u32, (i32, i32))>,
    /// Segments no longer part of a body, with whether their cells become debris or nothing, until
    /// the next rasterization clears them
    pub(crate) dropped_segments: Vec<(SoftBodySegment, bool)>,
}

impl SoftBodies {

    /// Creates an empty set of soft bodies
    pub(crate) fn new() -> Self {
        Self {
            set: SoftBodySet::new(),
            bodies: BTreeMap::new(),
            next_identifier: 1,
            origin: CellularTileCoordinates { x: 0, y: 0 },
            events: Vec::new(),
            tick: 0,
            environment_tags: Vec::new(),
            is_environment_tags_changed: false,
            tile_owners: HashMap::new(),
            dropped_segments: Vec::new(),
        }
    }

    /// Converts a scene position to tiles relative to the Rapier origin
    pub(crate) fn local_position(&self, position: ScenePosition) -> [f32; 2] {
        let (x, y): (f64, f64) = position.relative_position(ScenePosition {
            tile_x: self.origin.x,
            tile_y: self.origin.y,
            offset_x: 0,
            offset_y: 0,
        });
        [x as f32, y as f32]
    }

    /// Converts tiles relative to the Rapier origin to a scene position
    pub(crate) fn scene_position(&self, position: [f32; 2]) -> ScenePosition {
        let (x, y): (f64, f64) = (f64::from(position[0]), f64::from(position[1]));
        ScenePosition {
            tile_x: self.origin.x + x.floor() as i64,
            tile_y: self.origin.y + y.floor() as i64,
            offset_x: ((x - x.floor()) * ScenePosition::OFFSET_UNITS_PER_TILE) as u32,
            offset_y: ((y - y.floor()) * ScenePosition::OFFSET_UNITS_PER_TILE) as u32,
        }
    }

    /// Creates a soft body from a descriptor; rigid body anchors resolve through `rigid_body`
    pub(crate) fn spawn(
        &mut self,
        descriptor: &SoftBodyDescriptor,
        rapier: &mut SoftBodyRapier<'_>,
        rigid_body: impl Fn(crate::scenes::SceneRigidBody) -> Option<RigidBodyHandle>,
    ) -> SceneSoftBody {
        let positions: Vec<[f32; 2]> = descriptor.particles.iter()
            .map(|particle| self.local_position(particle.position))
            .collect();
        let masses: Vec<f32> = descriptor.particles.iter()
            .map(|particle| particle.mass.max(0.0001))
            .collect();
        let pinned: Vec<u32> = descriptor.anchors.iter()
            .filter(|anchor| matches!(anchor, SoftBodyAnchor::Cell { .. }))
            .map(|anchor| anchor.particle())
            .collect();
        let mut material = rapier2d::dynamics::SoftBodyMaterial::uniform(
            SpringCoefficients::new(descriptor.edge_frequency, 1.0),
        );
        material.bend_softness = SpringCoefficients::new(descriptor.bend_frequency, 1.0);
        material.tear_strain = descriptor.tear_strain;
        // pieces of a single segment are allowed, since a burning body falls apart that far
        material.min_piece = Some(1);
        if let Some(elasticity) = descriptor.elasticity {
            material.young_modulus = elasticity.young_modulus;
            material.poisson_ratio = elasticity.poisson_ratio;
            material.elastic_damping_ratio = elasticity.damping_ratio;
        }
        let mut builder: SoftBodyBuilder = SoftBodyBuilder::new(
            positions.iter().map(|position| Vector::new(position[0], position[1])).collect(),
        )
            .masses(masses)
            .edges(descriptor.edges.clone())
            .bend_edges(descriptor.bend_edges.clone())
            .surface(descriptor.surface.clone())
            .material(material)
            .pinned_particles(pinned)
            .particle_radius(descriptor.radius.max(0.001))
            .linear_damping(descriptor.damping.max(0.0))
            .gravity_scale(descriptor.gravity_scale)
            .volume_preservation(descriptor.preserves_area)
            // sleeping is decided here, after terrain contacts have settled the particles
            .can_sleep(false)
            // only actors meet the colliders: cells and rigid bodies are handled by `settle`
            .surface_collider(ColliderBuilder::polyline(Vec::new(), None).collision_groups(
                InteractionGroups::new(
                    // actors move past what is in this group, pushing it aside
                    Group::from_bits_retain(if descriptor.blocks_actors { COLLISION_GROUP_SOFT_BODY_SOLID } else { COLLISION_GROUP_SOFT_BODY }),
                    Group::from_bits_retain(COLLISION_GROUP_ACTOR),
                    InteractionTestMode::And,
                ),
            ));
        if !descriptor.touches_actors { builder = builder.no_surface_collider(); }
        if descriptor.elasticity.is_some() && !descriptor.triangles.is_empty() {
            builder = builder.cells(descriptor.triangles.clone())
                .cell_model(rapier2d::dynamics::SoftBodyCellModel::Corotational);
        }
        let handle: SoftBodyHandle = self.set.insert(builder, rapier.bodies, rapier.colliders);
        // an existing shape continues where it was, deformed and moving
        let mut settled: Vec<[f32; 2]> = positions;
        if descriptor.initial_state.len() == settled.len() {
            if let Some(soft_body) = self.set.get_mut(handle) {
                for (particle, (position, velocity)) in descriptor.initial_state.iter().enumerate() {
                    let local: [f32; 2] = {
                        let (x, y): (f64, f64) = position.relative_position(ScenePosition {
                            tile_x: self.origin.x,
                            tile_y: self.origin.y,
                            offset_x: 0,
                            offset_y: 0,
                        });
                        [x as f32, y as f32]
                    };
                    settled[particle] = local;
                    soft_body.set_particle_position(particle, Vector::new(local[0], local[1]));
                    soft_body.set_particle_velocity(particle, Vector::new(velocity[0], velocity[1]));
                }
            }
        }
        if let Some(soft_body) = self.set.get_mut(handle) {
            for anchor in &descriptor.anchors {
                if let SoftBodyAnchor::RigidBody { particle, rigid_body: carrier } = anchor
                    && let Some(carrier) = rigid_body(*carrier)
                {
                    soft_body.attach_particle(*particle as usize, carrier, rapier.bodies);
                }
            }
        }
        let identifier: SceneSoftBody = SceneSoftBody(self.next_identifier);
        self.next_identifier += 1;
        self.bodies.insert(identifier, SoftBody::new(handle, descriptor, settled));
        identifier
    }

    /// Removes a soft body, returning whether it existed
    pub(crate) fn remove(&mut self, body: SceneSoftBody, rapier: &mut SoftBodyRapier<'_>) -> bool {
        let Some(record) = self.bodies.remove(&body) else { return false; };
        self.drop_segments(record.segments.clone(), false);
        self.set.remove(
            record.handle,
            rapier.islands,
            rapier.bodies,
            rapier.colliders,
            rapier.impulse_joints,
            rapier.multibody_joints,
        );
        true
    }

    /// Hands segments to the next rasterization to clear
    pub(crate) fn drop_segments(&mut self, segments: Vec<SoftBodySegment>, is_debris: bool) {
        for segment in segments {
            for tile in segment.tiles.values() { self.tile_owners.remove(&tile.index); }
            self.dropped_segments.push((segment, is_debris));
        }
    }

    /// Removes the bodies whose cells changed and which became too small, their cells decomposing
    pub(crate) fn decompose_small(&mut self, rapier: &mut SoftBodyRapier<'_>) {
        let small: Vec<SceneSoftBody> = self.bodies.iter_mut()
            .filter_map(|(identifier, record)| {
                if !std::mem::take(&mut record.is_cell_changed) { return None; }
                let count: usize = record.segments.iter().map(SoftBodySegment::cell_count).sum();
                (count < record.minimum_cell_count).then_some(*identifier)
            })
            .collect();
        for body in small {
            let Some(record) = self.bodies.remove(&body) else { continue; };
            self.drop_segments(record.segments.clone(), true);
            self.set.remove(
                record.handle,
                rapier.islands,
                rapier.bodies,
                rapier.colliders,
                rapier.impulse_joints,
                rapier.multibody_joints,
            );
            self.events.push(SoftBodyEvent::Decomposed { body });
        }
    }

    /// Takes changed cells out of the segments holding them, given each as its GPU tile and cell,
    /// and returns the edges their segments no longer join
    pub(crate) fn remove_cells(
        &mut self,
        cells: &[(u32, u32)],
        material_table: &MaterialTable,
    ) -> Vec<(SceneSoftBody, u32, u32)> {
        let mut touched: BTreeSet<(SceneSoftBody, u32)> = BTreeSet::new();
        for (tile, cell) in cells {
            let Some((body, segment, coordinates)) = self.tile_owners.get(tile).copied() else { continue; };
            let Some(record) = self.bodies.get_mut(&body) else { continue; };
            let Some(segment_record) = record.segments.iter_mut().find(|held| held.index == segment)
            else { continue; };
            let local: (i32, i32) = (
                coordinates.0 * 8 + (*cell % 8) as i32,
                coordinates.1 * 8 + (*cell / 8) as i32,
            );
            if let Some(material) = segment_record.remove_cell(local) {
                // the cell is already whatever replaced it
                *material = MaterialIdentifier::EMPTY;
            }
            record.is_cell_changed = true;
            touched.insert((body, segment));
        }
        let mut parted: Vec<(SceneSoftBody, u32, u32)> = Vec::new();
        for (body, segment) in touched {
            let Some(record) = self.bodies.get_mut(&body) else { continue; };
            let Some(segment_record) = record.segments.iter_mut().find(|held| held.index == segment)
            else { continue; };
            let (loose, is_whole): (Vec<(i32, i32)>, bool) = segment_record.connectivity();
            if !is_whole {
                parted.push((body, segment_record.first, segment_record.second));
                continue;
            }
            // what the cut left hanging by nothing falls away as debris
            for cell in loose {
                if let Some(material) = segment_record.remove_cell(cell) {
                    *material = material_table.debris(*material).unwrap_or(MaterialIdentifier::EMPTY);
                }
            }
        }
        parted
    }

    /// Pushes awake particles out of the particles of other bodies nearby, waking what they touch,
    /// a bounded distance each step and never into solid cells
    fn separate_bodies(&mut self, terrain: &SoftBodyTerrain<'_>) {
        if self.bodies.len() < 2 || self.bodies.values().all(|record| record.is_sleeping) { return; }
        let widest: f32 = self.bodies.values().map(|record| record.radius).fold(0.0, f32::max);
        let bin_size: f32 = (widest * 2.0).max(0.5);
        let cells_per_tile: f32 = CELLULAR_TILE_SIZE as f32;
        let key = |position: [f32; 2]| -> (i32, i32) {
            ((position[0] / bin_size).floor() as i32, (position[1] / bin_size).floor() as i32)
        };
        let mut bins: HashMap<(i32, i32), Vec<(SceneSoftBody, [f32; 2], f32)>> = HashMap::new();
        let awake: Vec<([f32; 2], [f32; 2])> = self.bodies.values()
            .filter(|record| !record.is_sleeping)
            .map(|record| (record.bounds_minimum, record.bounds_maximum))
            .collect();
        for (identifier, record) in &self.bodies {
            if record.separation_grace > 0 { continue; }
            if !awake.iter().any(|(minimum, maximum)| record.bounds_near(*minimum, *maximum, bin_size)) {
                continue;
            }
            for position in &record.settled {
                bins.entry(key(*position)).or_default().push((*identifier, *position, record.radius));
            }
        }
        let mut woken: BTreeSet<SceneSoftBody> = BTreeSet::new();
        for (identifier, record) in &mut self.bodies {
            if record.is_sleeping { continue; }
            if record.separation_grace > 0 {
                record.separation_grace -= 1;
                continue;
            }
            let friction: f32 = record.friction;
            let Some(soft_body) = self.set.get_mut(record.handle) else { continue; };
            for particle in 0..record.settled.len().min(soft_body.num_particles()) {
                if record.held[particle] || soft_body.particles()[particle].is_pinned() { continue; }
                let mut position: [f32; 2] = record.settled[particle];
                let (bin_x, bin_y): (i32, i32) = key(position);
                let mut is_pushed: bool = false;
                for x in bin_x - 1..=bin_x + 1 {
                    for y in bin_y - 1..=bin_y + 1 {
                        let Some(others) = bins.get(&(x, y)) else { continue; };
                        for (other, at, radius) in others {
                            if other == identifier { continue; }
                            let reach: f32 = record.radius + radius;
                            let (dx, dy): (f32, f32) = (position[0] - at[0], position[1] - at[1]);
                            let distance: f32 = dx.hypot(dy);
                            if distance >= reach || distance < 0.0001 { continue; }
                            // each side of a touching pair moves half the way
                            let push: f32 = ((reach - distance) * 0.5)
                                .min(SOFT_BODY_SEPARATION_STEP) / distance;
                            position = [position[0] + dx * push, position[1] + dy * push];
                            is_pushed = true;
                            woken.insert(*other);
                        }
                    }
                }
                if is_pushed {
                    let start: [f32; 2] = record.settled[particle];
                    let position: [f32; 2] = if record.passes_terrain[particle] {
                        position
                    } else {
                        Self::sweep(terrain, start, position, friction, cells_per_tile)
                            .map_or(position, |(stopped, _)| stopped)
                    };
                    record.settled[particle] = position;
                    soft_body.set_particle_position(particle, Vector::new(position[0], position[1]));
                }
            }
        }
        for body in woken { self.wake(body); }
    }

    /// Returns the identifiers of the live bodies
    pub(crate) fn identifiers(&self) -> Vec<SceneSoftBody> {
        self.bodies.keys().copied().collect()
    }

    /// Returns a particle's position
    pub(crate) fn particle_position(
        &self,
        body: SceneSoftBody,
        particle: usize,
    ) -> Option<ScenePosition> {
        let position: [f32; 2] = *self.bodies.get(&body)?.settled.get(particle)?;
        Some(self.scene_position(position))
    }

    /// Returns a particle's velocity in tiles per second
    pub(crate) fn particle_velocity(&self, body: SceneSoftBody, particle: usize) -> Option<[f32; 2]> {
        let soft_body = self.set.get(self.bodies.get(&body)?.handle)?;
        if particle >= soft_body.num_particles() { return None; }
        let velocity: Vector = soft_body.particle_velocity(particle);
        Some([velocity.x, velocity.y])
    }

    /// Returns the rest length between neighboring particles along a body's edges from one particle
    /// to another, or `None` if no edges join them
    pub(crate) fn path_length(&self, body: SceneSoftBody, first: u32, second: u32) -> Option<f32> {
        let soft_body = self.set.get(self.bodies.get(&body)?.handle)?;
        let count: usize = soft_body.num_particles();
        if first as usize >= count || second as usize >= count { return None; }
        let mut distances: Vec<f32> = vec![f32::INFINITY; count];
        distances[first as usize] = 0.0;
        let mut is_changing: bool = true;
        while is_changing {
            is_changing = false;
            for edge in soft_body.edges().iter()
                .filter(|edge| edge.kind != SoftBodyEdgeKind::Bend)
            {
                let [a, b]: [u32; 2] = edge.vertices;
                for (from, to) in [(a, b), (b, a)] {
                    let reach: f32 = distances[from as usize] + edge.rest_length;
                    if reach < distances[to as usize] - 0.0001 {
                        distances[to as usize] = reach;
                        is_changing = true;
                    }
                }
            }
        }
        distances[second as usize].is_finite().then_some(distances[second as usize])
    }

    /// Returns the newest sample of the cell grid around a particle
    pub(crate) fn particle_environment(
        &self,
        body: SceneSoftBody,
        particle: usize,
    ) -> Option<SoftBodyEnvironment> {
        self.bodies.get(&body)?.environments.get(particle).copied()
    }

    /// Returns the number of particles in a body
    pub(crate) fn particle_count(&self, body: SceneSoftBody) -> Option<usize> {
        Some(self.bodies.get(&body)?.settled.len())
    }

    /// Returns a body's particles and how they are joined now, which cuts change
    pub(crate) fn particles(&self, body: SceneSoftBody) -> Option<SoftBodyParticles> {
        let record: &SoftBody = self.bodies.get(&body)?;
        let soft_body = self.set.get(record.handle)?;
        let count: usize = record.settled.len().min(soft_body.num_particles());
        Some(SoftBodyParticles {
            origin: ScenePosition { tile_x: self.origin.x, tile_y: self.origin.y, offset_x: 0, offset_y: 0 },
            positions: record.settled[..count].to_vec(),
            velocities: (0..count).map(|particle| {
                let velocity: Vector = soft_body.particle_velocity(particle);
                [velocity.x, velocity.y]
            }).collect(),
            contacts: record.contacts[..count].to_vec(),
            triangles: soft_body.cells().iter().map(|cell| cell.vertices).collect(),
            outline: soft_body.boundary().to_vec(),
            masses: (0..count).map(|particle| soft_body.particles()[particle].mass()).collect(),
        })
    }

    /// Cuts a body along a line between two positions: particles along the line split, opening a
    /// slit, and if that parts the body it comes apart, reported as `SoftBodyEvent::Torn`
    pub(crate) fn cut(
        &mut self,
        body: SceneSoftBody,
        start: ScenePosition,
        end: ScenePosition,
        rapier: &mut SoftBodyRapier<'_>,
    ) -> bool {
        let Some(handle) = self.bodies.get(&body).map(|record| record.handle) else { return false; };
        let (start, end): ([f32; 2], [f32; 2]) = (self.local_position(start), self.local_position(end));
        let blade: [Vector; 2] = [Vector::new(start[0], start[1]), Vector::new(end[0], end[1])];
        let Some(event) = self.set.cut(
            handle,
            &blade,
            rapier.islands,
            rapier.bodies,
            rapier.colliders,
            rapier.impulse_joints,
            rapier.multibody_joints,
        ) else { return false; };
        self.apply_tear(&event);
        // the pieces begin touching, and part without a kick if nothing pushes them apart at first
        for record in self.bodies.values_mut() {
            if record.handle == handle || event.pieces.iter().any(|piece| piece.soft_body == record.handle) {
                record.separation_grace = SOFT_BODY_CUT_GRACE;
            }
        }
        true
    }

    /// Returns the anchors still holding a body
    pub(crate) fn anchors(&self, body: SceneSoftBody) -> Option<&[SoftBodyAnchor]> {
        Some(&self.bodies.get(&body)?.anchors)
    }

    /// Returns whether a body is asleep
    pub(crate) fn is_sleeping(&self, body: SceneSoftBody) -> Option<bool> {
        Some(self.bodies.get(&body)?.is_sleeping)
    }

    /// Wakes a body
    pub(crate) fn wake(&mut self, body: SceneSoftBody) {
        let Some(record) = self.bodies.get_mut(&body) else { return; };
        Self::wake_record(&mut self.set, record);
    }

    /// Lets Rapier simulate a body again
    fn wake_record(set: &mut SoftBodySet, record: &mut SoftBody) {
        record.rest_steps = 0;
        if !record.is_sleeping { return; }
        record.is_sleeping = false;
        if let Some(soft_body) = set.get_mut(record.handle) { soft_body.set_enabled(true); }
    }

    /// Holds a particle so that it follows a scene position, or lets it go with `None`
    pub(crate) fn set_particle_target(
        &mut self,
        body: SceneSoftBody,
        particle: usize,
        target: Option<ScenePosition>,
    ) {
        let target: Option<[f32; 2]> = target.map(|position| self.local_position(position));
        let Some(record) = self.bodies.get_mut(&body) else { return; };
        if particle >= record.settled.len() { return; }
        Self::wake_record(&mut self.set, record);
        let is_anchored: bool = record.anchors.iter()
            .any(|anchor| anchor.particle() as usize == particle);
        let Some(soft_body) = self.set.get_mut(record.handle) else { return; };
        match target {
            Some(target) => {
                if !record.held[particle] { soft_body.set_particle_pinned(particle, true); }
                record.held[particle] = true;
                soft_body.set_particle_kinematic_target(particle, Vector::new(target[0], target[1]));
            }
            None => {
                record.held[particle] = false;
                if !is_anchored { soft_body.set_particle_pinned(particle, false); }
            }
        }
    }

    /// Sets whether a particle passes through solid cells
    pub(crate) fn set_particle_passes_terrain(
        &mut self,
        body: SceneSoftBody,
        particle: usize,
        passes: bool,
    ) {
        let Some(record) = self.bodies.get_mut(&body) else { return; };
        if let Some(held) = record.passes_terrain.get_mut(particle) { *held = passes; }
        if passes { if let Some(contact) = record.contacts.get_mut(particle) { *contact = [0.0; 2]; } }
    }

    /// Adds velocity to a particle and wakes its body
    pub(crate) fn add_particle_velocity(
        &mut self,
        body: SceneSoftBody,
        particle: usize,
        velocity: [f32; 2],
    ) {
        let Some(record) = self.bodies.get_mut(&body) else { return; };
        Self::wake_record(&mut self.set, record);
        let Some(soft_body) = self.set.get_mut(record.handle) else { return; };
        if particle >= soft_body.num_particles() { return; }
        let current: Vector = soft_body.particle_velocity(particle);
        soft_body.set_particle_velocity(particle, current + Vector::new(velocity[0], velocity[1]));
    }

    /// Adds an outward push to every particle near a scene position, falling off linearly to zero
    /// at a radius in tiles, in tiles per second at the center
    pub(crate) fn add_radial_velocity(&mut self, center: ScenePosition, radius: f32, speed: f32) {
        let center: [f32; 2] = self.local_position(center);
        for record in self.bodies.values_mut() {
            if !record.bounds_near(center, center, radius) { continue; }
            Self::wake_record(&mut self.set, record);
            let Some(soft_body) = self.set.get_mut(record.handle) else { continue; };
            let origin: Vector = Vector::new(center[0], center[1]);
            for particle in 0..soft_body.num_particles() {
                let offset: Vector = soft_body.particle_position(particle) - origin;
                let distance: f32 = offset.length();
                if distance >= radius || distance < 0.0001 { continue; }
                let push: Vector = offset * (speed * (1.0 - distance / radius) / distance);
                let velocity: Vector = soft_body.particle_velocity(particle);
                soft_body.set_particle_velocity(particle, velocity + push);
            }
        }
    }

    /// Returns the particle nearest a scene position within a distance in tiles
    pub(crate) fn nearest_particle(
        &self,
        position: ScenePosition,
        distance: f32,
    ) -> Option<(SceneSoftBody, usize, f32)> {
        let point: [f32; 2] = self.local_position(position);
        let mut best: Option<(SceneSoftBody, usize, f32)> = None;
        for (identifier, record) in &self.bodies {
            if !record.bounds_near(point, point, distance) { continue; }
            for (particle, candidate) in record.settled.iter().enumerate() {
                let separation: f32 = ((candidate[0] - point[0]).powi(2)
                    + (candidate[1] - point[1]).powi(2)).sqrt();
                if separation <= distance
                    && best.is_none_or(|(_, _, nearest)| separation < nearest)
                {
                    best = Some((*identifier, particle, separation));
                }
            }
        }
        best
    }

    /// Takes the events since the last call
    pub(crate) fn take_events(&mut self) -> Vec<SoftBodyEvent> { std::mem::take(&mut self.events) }

    /// Sets the material tags reported by particle environments, at most eight
    pub(crate) fn set_environment_tags(&mut self, tags: Vec<String>) {
        self.environment_tags = tags;
        self.environment_tags.truncate(8);
        self.is_environment_tags_changed = true;
    }

    /// Tears the edge joining two neighboring particles, returning whether it existed
    pub(crate) fn tear_between(
        &mut self,
        body: SceneSoftBody,
        first: u32,
        second: u32,
        rapier: &mut SoftBodyRapier<'_>,
    ) -> bool {
        let Some(handle) = self.bodies.get(&body).map(|record| record.handle) else { return false; };
        let Some(soft_body) = self.set.get(handle) else { return false; };
        let Some(edge) = soft_body.edges().iter().position(|edge| {
            let [a, b]: [u32; 2] = edge.vertices;
            (a == first && b == second) || (a == second && b == first)
        }) else { return false; };
        let event: Option<SoftBodyTearEvent> = self.set.tear(
            handle,
            &[edge as u32],
            &[],
            rapier.islands,
            rapier.bodies,
            rapier.colliders,
            rapier.impulse_joints,
            rapier.multibody_joints,
        );
        let Some(event) = event else { return false; };
        self.apply_tear(&event);
        true
    }

    /// Divides the records of a torn body between the bodies it became
    pub(crate) fn apply_tear(&mut self, event: &SoftBodyTearEvent) {
        let Some(identifier) = self.bodies.iter()
            .find(|(_, record)| record.handle == event.soft_body)
            .map(|(identifier, _)| *identifier)
        else { return; };
        let Some(mut whole) = self.bodies.remove(&identifier) else { return; };
        if whole.settled.is_empty() {
            self.bodies.insert(identifier, whole);
            return;
        }
        // every segment goes with one piece, or falls apart with the edge that tore
        let mut segments: Vec<Option<SoftBodySegment>> =
            std::mem::take(&mut whole.segments).into_iter().map(Some).collect();
        // copies of split particles start as what they copied
        let mut sources: Vec<u32> = (0..whole.settled.len() as u32).collect();
        for (copy, source) in &event.split_particles {
            let copy: usize = *copy as usize;
            let source: usize = *source as usize;
            if copy >= sources.len() { sources.resize(copy + 1, source as u32); }
            sources[copy] = source as u32;
        }
        let bodies: Vec<(SoftBodyHandle, Vec<u32>)> = if event.pieces.is_empty() {
            vec![(event.soft_body, (0..sources.len() as u32).collect())]
        } else {
            event.pieces.iter()
                .map(|piece| (piece.soft_body, piece.particles.clone()))
                .collect()
        };
        whole.is_sleeping = false;
        let mut pieces: Vec<(SceneSoftBody, Vec<u32>)> = Vec::with_capacity(bodies.len());
        for (index, (handle, particles)) in bodies.into_iter().enumerate() {
            let piece_identifier: SceneSoftBody = if index == 0 { identifier } else {
                let fresh: SceneSoftBody = SceneSoftBody(self.next_identifier);
                self.next_identifier += 1;
                fresh
            };
            // a body cut again before an earlier cut's event arrives can name particles this record
            // does not know yet; those take the nearest known particle rather than panicking, and
            // the next event brings the record up to date
            let last: u32 = whole.settled.len().saturating_sub(1) as u32;
            let former: Vec<u32> = particles.iter()
                .map(|particle| sources.get(*particle as usize).copied().unwrap_or(last).min(last))
                .collect();
            let at = |old: u32| -> Option<u32> {
                // the piece's own copy of a particle, by its index after the tear
                particles.iter().position(|particle| *particle == old)
                    .or_else(|| {
                        // a split particle's copy may stand for it in this piece
                        former.iter().position(|source| *source == old)
                    })
                    .map(|position| position as u32)
            };
            let mut record: SoftBody = whole.clone();
            record.handle = handle;
            record.settled = former.iter().map(|source| whole.settled[*source as usize]).collect();
            record.accelerations = former.iter().map(|source| whole.accelerations[*source as usize]).collect();
            record.environments = former.iter().map(|source| whole.environments[*source as usize]).collect();
            record.held = former.iter().map(|source| whole.held[*source as usize]).collect();
            record.contacts = former.iter().map(|source| whole.contacts[*source as usize]).collect();
            record.passes_terrain =
                former.iter().map(|source| whole.passes_terrain[*source as usize]).collect();
            record.surface_coordinates =
                former.iter().map(|source| whole.surface_coordinates[*source as usize]).collect();
            for held in &mut segments {
                let Some(segment) = held.as_ref() else { continue; };
                let is_torn: bool = event.torn_edges.iter().any(|edge| {
                    (edge[0] == segment.first && edge[1] == segment.second)
                        || (edge[1] == segment.first && edge[0] == segment.second)
                });
                if is_torn { continue; }
                let (Some(first), Some(second)) = (at(segment.first), at(segment.second)) else { continue; };
                let Some(mut segment) = held.take() else { continue; };
                segment.first = first;
                segment.second = second;
                for (coordinates, tile) in &segment.tiles {
                    self.tile_owners.insert(tile.index, (piece_identifier, segment.index, *coordinates));
                }
                record.segments.push(segment);
            }
            record.is_cell_changed = true;
            record.anchors = whole.anchors.iter().filter_map(|anchor: &SoftBodyAnchor| {
                let particle: u32 = at(anchor.particle())?;
                Some(match *anchor {
                    SoftBodyAnchor::Cell { cell, .. } => SoftBodyAnchor::Cell { particle, cell },
                    SoftBodyAnchor::RigidBody { rigid_body, .. } =>
                        SoftBodyAnchor::RigidBody { particle, rigid_body },
                })
            }).collect();
            record.rest_steps = 0;
            record.update_bounds();
            self.bodies.insert(piece_identifier, record);
            pieces.push((piece_identifier, former));
        }
        self.drop_segments(segments.into_iter().flatten().collect(), true);
        self.events.push(SoftBodyEvent::Torn { body: identifier, pieces });
    }

    /// Moves the frame of every particle to a new Rapier origin
    pub(crate) fn move_origin(&mut self, origin: CellularTileCoordinates) {
        if origin == self.origin { return; }
        let shift: [f32; 2] = [
            (self.origin.x - origin.x) as f32,
            (self.origin.y - origin.y) as f32,
        ];
        self.origin = origin;
        for record in self.bodies.values_mut() {
            for position in record.settled.iter_mut() {
                position[0] += shift[0];
                position[1] += shift[1];
            }
            record.update_bounds();
            let Some(soft_body) = self.set.get_mut(record.handle) else { continue; };
            for particle in 0..soft_body.num_particles() {
                let position: Vector = soft_body.particle_position(particle);
                soft_body.set_particle_position(
                    particle,
                    position + Vector::new(shift[0], shift[1]),
                );
            }
        }
    }

    /// Releases cell anchors whose cells stopped being solid, reporting them
    pub(crate) fn update_cell_anchors(&mut self, terrain: &SoftBodyTerrain<'_>) {
        let origin_cell: [i64; 2] = [
            self.origin.x * CELLULAR_TILE_SIZE,
            self.origin.y * CELLULAR_TILE_SIZE,
        ];
        for (identifier, record) in self.bodies.iter_mut() {
            let mut index: usize = 0;
            while index < record.anchors.len() {
                let SoftBodyAnchor::Cell { cell, particle } = record.anchors[index]
                else { index += 1; continue; };
                let local: [i64; 2] = [cell.x - origin_cell[0], cell.y - origin_cell[1]];
                if terrain.contains_cell(local) && !terrain.is_solid(local) {
                    let anchor: SoftBodyAnchor = record.anchors.swap_remove(index);
                    Self::wake_record(&mut self.set, record);
                    if !record.held[particle as usize]
                        && let Some(soft_body) = self.set.get_mut(record.handle)
                    { soft_body.set_particle_pinned(particle as usize, false); }
                    self.events.push(SoftBodyEvent::AnchorLost { body: *identifier, anchor });
                } else {
                    index += 1;
                }
            }
        }
    }

    /// Wakes the sleeping bodies whose bounds a moving box touches
    pub(crate) fn wake_near(&mut self, minimum: [f32; 2], maximum: [f32; 2]) {
        for record in self.bodies.values_mut() {
            if record.is_sleeping && record.bounds_near(minimum, maximum, record.radius + 0.25) {
                Self::wake_record(&mut self.set, record);
            }
        }
    }

    /// Applies the cell grid's accelerations for one step and runs after Rapier's step: stops
    /// particles at the first solid cell along their motion, slides them along it, and puts bodies
    /// that have come to rest to sleep. `contact` pushes a position out of rigid bodies
    pub(crate) fn settle(
        &mut self,
        terrain: &SoftBodyTerrain<'_>,
        mut contact: impl FnMut([f32; 2], f32, f32) -> Option<[f32; 2]>,
    ) {
        let cells_per_tile: f32 = CELLULAR_TILE_SIZE as f32;
        for record in self.bodies.values_mut() {
            if record.is_sleeping { continue; }
            let Some(soft_body) = self.set.get_mut(record.handle) else { continue; };
            let mut fastest: f32 = 0.0;
            let mut is_pushed: bool = false;
            for particle in 0..record.settled.len().min(soft_body.num_particles()) {
                record.contacts[particle] = [0.0; 2];
                let start: [f32; 2] = record.settled[particle];
                let moved: Vector = soft_body.particle_position(particle);
                let mut end: [f32; 2] = [moved.x, moved.y];
                let is_free: bool = !record.held[particle] && !record.anchors.iter().any(|anchor| {
                    anchor.particle() as usize == particle
                });
                if !is_free {
                    record.settled[particle] = end;
                    if record.held[particle] { is_pushed = true; }
                    continue;
                }
                if record.passes_terrain[particle] {
                    record.settled[particle] = end;
                    let travel: f32 = ((end[0] - start[0]).powi(2) + (end[1] - start[1]).powi(2)).sqrt();
                    fastest = fastest.max(travel / SOFT_BODY_STEP_SECONDS);
                    continue;
                }
                let mut velocity: Vector = soft_body.particle_velocity(particle);
                let mut is_changed: bool = false;
                if let Some(pushed) = contact(end, record.radius, soft_body_mass(soft_body, particle)) {
                    end = pushed;
                    is_changed = true;
                }
                if let Some((stopped, axis)) =
                    Self::sweep(terrain, start, end, record.friction, cells_per_tile)
                {
                    // it was stopped moving this way, or pushed out of a cell that appeared
                    let moved_along: f32 = end[axis] - start[axis];
                    record.contacts[particle] = if axis == 0 {
                        [-moved_along.signum(), 0.0]
                    } else if moved_along > 0.0 { [0.0, -1.0] } else { [0.0, 1.0] };
                    end = stopped;
                    if axis == 0 { velocity.x = 0.0; velocity.y *= 1.0 - record.friction; }
                    else { velocity.y = 0.0; velocity.x *= 1.0 - record.friction; }
                    is_changed = true;
                }
                let acceleration: [f32; 2] = record.accelerations[particle];
                if acceleration != [0.0; 2] {
                    velocity.x += acceleration[0] * SOFT_BODY_STEP_SECONDS;
                    velocity.y += acceleration[1] * SOFT_BODY_STEP_SECONDS;
                    is_changed = true;
                    if acceleration[0].abs() + acceleration[1].abs() > 2.0 { is_pushed = true; }
                }
                if is_changed {
                    soft_body.set_particle_position(particle, Vector::new(end[0], end[1]));
                    soft_body.set_particle_velocity(particle, velocity);
                }
                record.settled[particle] = end;
                let travel: f32 = ((end[0] - start[0]).powi(2) + (end[1] - start[1]).powi(2)).sqrt();
                fastest = fastest.max(travel / SOFT_BODY_STEP_SECONDS);
            }
            record.update_bounds();
            if fastest <= SOFT_BODY_REST_SPEED && !is_pushed {
                record.rest_steps += 1;
                if record.rest_steps >= SOFT_BODY_REST_STEP_COUNT {
                    record.is_sleeping = true;
                    for particle in 0..soft_body.num_particles() {
                        if !soft_body.particles()[particle].is_pinned() {
                            soft_body.set_particle_velocity(particle, Vector::ZERO);
                        }
                    }
                    soft_body.set_enabled(false);
                }
            } else {
                record.rest_steps = 0;
            }
        }
        self.separate_bodies(terrain);
        self.tick = self.tick.wrapping_add(1);
    }

    /// Returns where a particle moving between two positions stops at the first solid cell along
    /// its way, keeping its slide along the cell less friction, with the axis it stopped along
    fn sweep(
        terrain: &SoftBodyTerrain<'_>,
        start: [f32; 2],
        end: [f32; 2],
        friction: f32,
        cells_per_tile: f32,
    ) -> Option<([f32; 2], usize)> {
        let mut cell: [i64; 2] = [
            (start[0] * cells_per_tile).floor() as i64,
            (start[1] * cells_per_tile).floor() as i64,
        ];
        let target: [i64; 2] = [
            (end[0] * cells_per_tile).floor() as i64,
            (end[1] * cells_per_tile).floor() as i64,
        ];
        // terrain that appeared around the particle pushes it out to the nearest open cell
        if terrain.is_solid(cell) {
            return terrain.nearest_open_position(cell, start).map(|open| (open, 1));
        }
        if cell == target { return None; }
        let direction: [f32; 2] = [end[0] - start[0], end[1] - start[1]];
        let step: [i64; 2] = [direction[0].signum() as i64, direction[1].signum() as i64];
        let inverse: [f32; 2] = [
            if direction[0] != 0.0 { 1.0 / direction[0] } else { f32::INFINITY },
            if direction[1] != 0.0 { 1.0 / direction[1] } else { f32::INFINITY },
        ];
        let cell_size: f32 = 1.0 / cells_per_tile;
        let boundary = |cell: i64, step: i64| -> f32 {
            (cell + i64::from(step > 0)) as f32 * cell_size
        };
        let mut next: [f32; 2] = [
            if step[0] != 0 { (boundary(cell[0], step[0]) - start[0]) * inverse[0] } else { f32::INFINITY },
            if step[1] != 0 { (boundary(cell[1], step[1]) - start[1]) * inverse[1] } else { f32::INFINITY },
        ];
        let advance: [f32; 2] = [cell_size * inverse[0].abs(), cell_size * inverse[1].abs()];
        // no more cells than a straight walk between the two can visit
        for _ in 0..=((target[0] - cell[0]).abs() + (target[1] - cell[1]).abs()) {
            if cell == target { return None; }
            let axis: usize = if next[0] < next[1] { 0 } else { 1 };
            let mut candidate: [i64; 2] = cell;
            candidate[axis] += step[axis];
            if terrain.is_solid(candidate) {
                let other: usize = 1 - axis;
                let mut stopped: [f32; 2] = end;
                stopped[axis] = boundary(cell[axis], step[axis])
                    - step[axis] as f32 * SOFT_BODY_TERRAIN_MARGIN;
                stopped[other] = start[other] + (end[other] - start[other]) * (1.0 - friction);
                let mut slid: [i64; 2] = cell;
                slid[other] = (stopped[other] * cells_per_tile).floor() as i64;
                if terrain.is_solid(slid) { stopped[other] = start[other]; }
                return Some((stopped, axis));
            }
            cell = candidate;
            next[axis] += advance[axis];
        }
        None
    }

    /// Chooses the particles sampled this step: every other particle of awake bodies, and a few of
    /// each sleeping body in turn, inside a region, up to the capacity of the sampling pass
    pub(crate) fn choose_samples(
        &self,
        region: CellularTileRegion,
    ) -> (Vec<[u32; 2]>, Vec<(SceneSoftBody, u32, u32)>) {
        let mut cells: Vec<[u32; 2]> = Vec::new();
        let mut records: Vec<(SceneSoftBody, u32, u32)> = Vec::new();
        let region_x: f32 = (region.bottom_left.x - self.origin.x) as f32;
        let region_y: f32 = (region.bottom_left.y - self.origin.y) as f32;
        let (width, height): (f32, f32) = (region.width() as f32, region.height() as f32);
        for (identifier, record) in &self.bodies {
            let stride: usize = if record.is_sleeping { 4 } else { 2 };
            let phase: usize = (self.tick as usize).wrapping_add(identifier.0 as usize) % stride;
            let mut particle: usize = phase;
            while particle < record.settled.len() {
                let position: [f32; 2] = record.settled[particle];
                let x: f32 = position[0] - region_x;
                let y: f32 = position[1] - region_y;
                if x >= 0.0 && y >= 0.0 && x < width && y < height {
                    if cells.len() >= SOFT_BODY_SAMPLE_COUNT_MAXIMUM { return (cells, records); }
                    cells.push([
                        (x * CELLULAR_TILE_SIZE as f32) as u32,
                        (y * CELLULAR_TILE_SIZE as f32) as u32,
                    ]);
                    records.push((*identifier, particle as u32, stride as u32));
                }
                particle += stride;
            }
        }
        (cells, records)
    }

    /// Records one completed sample of a particle's environment, waking a sleeping body it pushes
    pub(crate) fn apply_sample(
        &mut self,
        body: SceneSoftBody,
        particle: u32,
        spread: u32,
        environment: SoftBodyEnvironment,
        gravity: f32,
    ) {
        let Some(record) = self.bodies.get_mut(&body) else { return; };
        let particle: usize = particle as usize;
        if particle >= record.settled.len() { return; }
        let velocity: [f32; 2] = self.set.get(record.handle)
            .filter(|soft_body| particle < soft_body.num_particles())
            .map_or([0.0; 2], |soft_body| {
                let velocity: Vector = soft_body.particle_velocity(particle);
                [velocity.x, velocity.y]
            });
        record.apply_environment(particle, environment, spread as usize, gravity, velocity);
        let acceleration: [f32; 2] = record.accelerations[particle];
        if record.is_sleeping && acceleration[0].abs() + acceleration[1].abs() > 8.0 {
            Self::wake_record(&mut self.set, record);
        }
    }

}

/// Returns the mass of a particle of a Rapier soft body
fn soft_body_mass(soft_body: &rapier2d::dynamics::SoftBody, particle: usize) -> f32 {
    soft_body.particles()[particle].mass()
}

#[cfg(test)]
mod tests {

    use super::{
        SceneSoftBody,
        SoftBodies,
        SoftBodyEvent,
        SoftBodyRapier,
        SoftBodyTearEvents,
    };
    use crate::scenes::{
        cellular_tiles::{
            CellCoordinates,
            CellularTileCoordinates,
            CellularTileRegion,
        },
        collisions::CollisionCellularTile,
        soft_bodies::{
            SoftBodyAnchor,
            SoftBodyDescriptor,
            SoftBodyParticleDescriptor,
            SoftBodyTerrain,
        },
        ScenePosition,
    };
    use rapier2d::{
        dynamics::{
            CCDSolver,
            ImpulseJointSet,
            IntegrationParameters,
            IslandManager,
            MultibodyJointSet,
            RigidBodySet,
        },
        geometry::{BroadPhaseBvh, ColliderSet, NarrowPhase},
        math::Vector,
        pipeline::PhysicsPipeline,
    };

    /// A Rapier world holding soft bodies against a region whose top tile row is solid
    struct World {
        soft_bodies: SoftBodies,
        region: CellularTileRegion,
        tiles: Vec<CollisionCellularTile>,
        bodies: RigidBodySet,
        colliders: ColliderSet,
        islands: IslandManager,
        impulse_joints: ImpulseJointSet,
        multibody_joints: MultibodyJointSet,
        broad_phase: BroadPhaseBvh,
        narrow_phase: NarrowPhase,
        pipeline: PhysicsPipeline,
        ccd: CCDSolver,
        parameters: IntegrationParameters,
    }

    impl World {

        fn new() -> Self {
            let mut tiles: Vec<CollisionCellularTile> =
                vec![CollisionCellularTile::from_u64(0); 32 * 32];
            for x in 0..32 { tiles[31 * 32 + x] = CollisionCellularTile::from_u64(u64::MAX); }
            Self {
                soft_bodies: SoftBodies::new(),
                region: CellularTileRegion {
                    bottom_left: CellularTileCoordinates { x: 0, y: 0 },
                    top_right: CellularTileCoordinates { x: 31, y: 31 },
                },
                tiles,
                bodies: RigidBodySet::new(),
                colliders: ColliderSet::new(),
                islands: IslandManager::new(),
                impulse_joints: ImpulseJointSet::new(),
                multibody_joints: MultibodyJointSet::new(),
                broad_phase: BroadPhaseBvh::new(),
                narrow_phase: NarrowPhase::new(),
                pipeline: PhysicsPipeline::new(),
                ccd: CCDSolver::new(),
                parameters: IntegrationParameters::default(),
            }
        }

        fn spawn(&mut self, descriptor: &SoftBodyDescriptor) -> SceneSoftBody {
            let mut rapier: SoftBodyRapier<'_> = SoftBodyRapier {
                bodies: &mut self.bodies,
                colliders: &mut self.colliders,
                islands: &mut self.islands,
                impulse_joints: &mut self.impulse_joints,
                multibody_joints: &mut self.multibody_joints,
            };
            self.soft_bodies.spawn(descriptor, &mut rapier, |_| None)
        }

        fn step(&mut self) {
            let terrain: SoftBodyTerrain<'_> = SoftBodyTerrain::new(self.region, [0, 0], &self.tiles);
            self.soft_bodies.update_cell_anchors(&terrain);
            let events: SoftBodyTearEvents = SoftBodyTearEvents::default();
            self.pipeline.step(
                Vector::new(0.0, -50.0),
                &self.parameters,
                &mut self.islands,
                &mut self.broad_phase,
                &mut self.narrow_phase,
                &mut self.bodies,
                &mut self.colliders,
                &mut self.impulse_joints,
                &mut self.multibody_joints,
                &mut self.soft_bodies.set,
                &mut self.ccd,
                &(),
                &events,
            );
            for event in events.0.lock().unwrap().iter() { self.soft_bodies.apply_tear(event); }
            self.soft_bodies.settle(&terrain, |_, _, _| None);
        }

        fn tear(&mut self, body: SceneSoftBody, first: u32, second: u32) -> bool {
            let mut rapier: SoftBodyRapier<'_> = SoftBodyRapier {
                bodies: &mut self.bodies,
                colliders: &mut self.colliders,
                islands: &mut self.islands,
                impulse_joints: &mut self.impulse_joints,
                multibody_joints: &mut self.multibody_joints,
            };
            self.soft_bodies.tear_between(body, first, second, &mut rapier)
        }

    }

    fn position(x: f64, y: f64) -> ScenePosition {
        ScenePosition {
            tile_x: x.floor() as i64,
            tile_y: y.floor() as i64,
            offset_x: ((x - x.floor()) * ScenePosition::OFFSET_UNITS_PER_TILE) as u32,
            offset_y: ((y - y.floor()) * ScenePosition::OFFSET_UNITS_PER_TILE) as u32,
        }
    }

    /// A chain hanging from the ceiling at a column
    fn hanging_chain(x: f64, count: usize, spacing: f64) -> SoftBodyDescriptor {
        let particles: Vec<SoftBodyParticleDescriptor> = (0..count)
            .map(|index| SoftBodyParticleDescriptor {
                position: position(x, 30.99 - index as f64 * spacing),
                mass: 0.05,
            })
            .collect();
        let mut descriptor: SoftBodyDescriptor = SoftBodyDescriptor::chain(particles);
        descriptor.anchors.push(SoftBodyAnchor::Cell {
            particle: 0,
            cell: CellCoordinates { x: (x * 8.0).floor() as i64, y: 248 },
        });
        descriptor
    }

    #[test]
    fn chain_hangs_still_and_sleeps() {
        let mut world: World = World::new();
        let body: SceneSoftBody = world.spawn(&hanging_chain(10.5, 24, 0.25));
        for _ in 0..600 { world.step(); }
        assert_eq!(world.soft_bodies.is_sleeping(body), Some(true));
        let record = &world.soft_bodies.bodies[&body];
        let length: f32 = record.settled.windows(2)
            .map(|pair| ((pair[1][0] - pair[0][0]).powi(2)
                + (pair[1][1] - pair[0][1]).powi(2)).sqrt())
            .sum();
        assert!((length - 0.25 * 23.0).abs() < 0.3, "stretched to {length}");
    }

    #[test]
    fn pushed_chain_swings_and_settles() {
        let mut world: World = World::new();
        let body: SceneSoftBody = world.spawn(&hanging_chain(10.5, 24, 0.25));
        for _ in 0..60 { world.step(); }
        world.soft_bodies.add_particle_velocity(body, 23, [60.0, 0.0]);
        let mut furthest: f32 = 0.0;
        for _ in 0..120 {
            world.step();
            furthest = furthest.max(world.soft_bodies.bodies[&body].settled[23][0] - 10.5);
        }
        assert!(furthest > 0.3, "the push should swing the chain, moved {furthest}");
        for _ in 0..1800 { world.step(); }
        let end: f32 = world.soft_bodies.bodies[&body].settled[23][0];
        assert!((end - 10.5).abs() < 0.75, "the chain should settle, ended at {end}");
    }

    #[test]
    fn particles_never_enter_solid_cells() {
        let mut world: World = World::new();
        let body: SceneSoftBody = world.spawn(&hanging_chain(10.5, 24, 0.25));
        for _ in 0..30 { world.step(); }
        for particle in 1..24 { world.soft_bodies.add_particle_velocity(body, particle, [0.0, 90.0]); }
        for _ in 0..240 {
            world.step();
            let terrain = SoftBodyTerrain::new(world.region, [0, 0], &world.tiles);
            for position in &world.soft_bodies.bodies[&body].settled[1..] {
                let cell: [i64; 2] = [
                    (position[0] * 8.0).floor() as i64,
                    (position[1] * 8.0).floor() as i64,
                ];
                assert!(!terrain.is_solid(cell), "particle entered rock at {cell:?}");
            }
        }
    }

    #[test]
    fn chain_rests_on_a_floor_without_entering_it() {
        let mut world: World = World::new();
        for x in 0..32 { world.tiles[x] = CollisionCellularTile::from_u64(u64::MAX); }
        let particles: Vec<SoftBodyParticleDescriptor> = (0..20)
            .map(|index| SoftBodyParticleDescriptor {
                position: position(5.0 + f64::from(index) * 0.25, 6.0),
                mass: 0.05,
            })
            .collect();
        let body: SceneSoftBody = world.spawn(&SoftBodyDescriptor::chain(particles));
        for _ in 0..900 { world.step(); }
        let record = &world.soft_bodies.bodies[&body];
        for position in &record.settled {
            assert!(position[1] >= 1.0, "particle sank to {position:?}");
            assert!(position[1] < 1.6, "particle should rest on the floor at {position:?}");
        }
        assert_eq!(world.soft_bodies.is_sleeping(body), Some(true), "a resting chain should sleep");
    }

    #[test]
    fn removing_the_anchor_cell_lets_go_and_reports_it() {
        let mut world: World = World::new();
        let body: SceneSoftBody = world.spawn(&hanging_chain(10.5, 12, 0.25));
        for _ in 0..30 { world.step(); }
        let before: f32 = world.soft_bodies.bodies[&body].settled[0][1];
        for x in 0..32 { world.tiles[31 * 32 + x] = CollisionCellularTile::from_u64(0); }
        for _ in 0..30 { world.step(); }
        assert!(world.soft_bodies.take_events().iter()
            .any(|event| matches!(event, SoftBodyEvent::AnchorLost { .. })));
        assert!(world.soft_bodies.bodies[&body].settled[0][1] < before - 1.0, "it should fall");
    }

    #[test]
    fn tearing_splits_into_a_held_piece_and_a_falling_piece() {
        let mut world: World = World::new();
        let body: SceneSoftBody = world.spawn(&hanging_chain(10.5, 12, 0.25));
        for _ in 0..30 { world.step(); }
        assert!(world.tear(body, 5, 6));
        let events: Vec<SoftBodyEvent> = world.soft_bodies.take_events();
        let SoftBodyEvent::Torn { pieces, .. } = &events[0] else { panic!("no tear: {events:?}"); };
        assert_eq!(pieces.len(), 2);
        for _ in 0..60 { world.step(); }
        let holding: Vec<f32> = pieces.iter()
            .map(|(piece, _)| world.soft_bodies.bodies[piece].settled[0][1])
            .collect();
        let anchored: usize = pieces.iter()
            .filter(|(piece, _)| !world.soft_bodies.bodies[piece].anchors.is_empty())
            .count();
        assert_eq!(anchored, 1, "one piece keeps the anchor");
        let lowest: f32 = pieces.iter()
            .flat_map(|(piece, _)| world.soft_bodies.bodies[piece].settled.iter().map(|p| p[1]))
            .fold(f32::INFINITY, f32::min);
        assert!(lowest < 25.0, "the lower piece should fall, lowest {lowest}, tops {holding:?}");
    }

    #[test]
    fn a_chain_can_be_consumed_edge_by_edge_from_the_bottom() {
        let mut world: World = World::new();
        let mut body: SceneSoftBody = world.spawn(&hanging_chain(10.5, 12, 0.25));
        for _ in 0..30 { world.step(); }
        for _ in 0..10 {
            let count: u32 = world.soft_bodies.particle_count(body).unwrap() as u32;
            if count < 2 { break; }
            let is_torn: bool = world.tear(body, count - 2, count - 1);
            assert!(is_torn, "the last edge of {count} particles should tear");
            for event in world.soft_bodies.take_events() {
                let SoftBodyEvent::Torn { pieces, .. } = event else { continue; };
                body = pieces.iter()
                    .find(|(piece, _)| !world.soft_bodies.bodies[piece].anchors.is_empty())
                    .expect("one piece stays anchored").0;
            }
            for _ in 0..10 { world.step(); }
        }
        assert!(world.soft_bodies.particle_count(body).unwrap() <= 3);
    }

    #[test]
    fn closed_loops_stay_whole_and_keep_their_area() {
        let mut world: World = World::new();
        for x in 0..32 { world.tiles[x] = CollisionCellularTile::from_u64(u64::MAX); }
        let count: u32 = 16;
        let radius: f64 = 2.0;
        let particles: Vec<SoftBodyParticleDescriptor> = (0..count).map(|index| {
            let angle: f64 = f64::from(index) / f64::from(count) * std::f64::consts::TAU;
            SoftBodyParticleDescriptor {
                position: position(10.0 + radius * angle.cos(), 8.0 + radius * angle.sin()),
                mass: 0.1,
            }
        }).collect();
        let mut descriptor: SoftBodyDescriptor = SoftBodyDescriptor::chain(particles);
        descriptor.edges = (0..count).map(|index| [index, (index + 1) % count]).collect();
        descriptor.surface = descriptor.edges.clone();
        descriptor.bend_edges = (0..count).map(|index| [index, (index + 2) % count]).collect();
        descriptor.preserves_area = true;
        let body: SceneSoftBody = world.spawn(&descriptor);
        let area = |settled: &[[f32; 2]]| -> f32 {
            (0..settled.len()).map(|index| {
                let (a, b) = (settled[index], settled[(index + 1) % settled.len()]);
                a[0] * b[1] - b[0] * a[1]
            }).sum::<f32>().abs() * 0.5
        };
        let before: f32 = area(&world.soft_bodies.bodies[&body].settled);
        for _ in 0..600 { world.step(); }
        let record = &world.soft_bodies.bodies[&body];
        let after: f32 = area(&record.settled);
        assert!((after / before - 1.0).abs() < 0.25, "area went from {before} to {after}");
        assert!(record.settled.iter().all(|position| position[1] >= 1.0), "it sank into the floor");
    }

}
