// Copyright Rob Gage 2026

use super::{
    cellular_tiles::{
        CellCoordinates,
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
    collisions::{Collider, Collision, ColliderShape},
    edits::{
        Edit,
        EditError,
        EditStreamingState,
    },
    actor_sensing::{
        is_valid_configuration,
        ActorSensingPass,
        SceneActorSensing,
        SceneActorSensingConfiguration,
    },
    chemistry::Chemistry,
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
use super::soft_bodies::{
    SceneSoftBody,
    SceneTerrain,
    SoftBodyAnchor,
    SoftBodyDescriptor,
    SoftBodyEnvironment,
    SoftBodyEvent,
    SoftBodyParticles,
    SoftBodySamplingPass,
};
use super::{
    scene_backgrounds::SceneBackgrounds,
    scene_material_surfaces::{MaterialSurface, SceneMaterialSurfaces},
};
use engine_graphics::sprites::SpriteSheet;
use crate::{
    actors::{
        components::{
            ActorCameraComponent,
            ActorCollisionComponent,
            ActorIdentityComponent,
            ActorKinematicsPositionComponent,
            ActorLightComponent,
            ActorOverlayComponent,
            ActorOverlayShape,
            ActorParallaxComponent,
            ActorPawnControlComponent,
            ActorPersistenceComponent,
            ActorShaderComponent,
            ActorSoftBodyComponent,
            ActorSoftBodyImpulse,
            ActorSpritesComponent,
        },
        Actor,
        ActorError,
        ActorRegistry,
    },
    materials::MaterialTable,
};
use engine_graphics::world::scenes::{
    LightingQuality,
    SceneGraphics,
    SceneGraphicsCellular,
    SceneGraphicsDecorationMover,
    SceneGraphicsDecorations,
    SceneGraphicsFluid,
    SceneGraphicsGas,
    SceneGraphicsLight,
    SceneGraphicsLighting,
    SceneGraphicsOverlay,
    SceneGraphicsOverlayShape,
    SceneGraphicsParallax,
    SceneGraphicsPressure,
    SceneGraphicsShaded,
    SceneGraphicsSprites,
    SceneGraphicsTemperature,
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

/// Tiles from the edge of the simulated region at which an actor's soft body is taken away, before
/// it reaches what is not there to stand on
const ACTOR_SOFT_BODY_REGION_MARGIN: f32 = 2.0;

/// Curve points per particle along the outline of a body drawn with a shader
const ACTOR_SHADER_CURVE_STEPS: usize = 3;

/// Stable identifier for a live cellular rigid body in one scene
#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq)]
pub struct SceneRigidBody(pub(crate) u64);

/// A rigid body point held by a pawn's tether, with the tether length's gradient at that point
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct ScenePawnTetherBody {
    /// The rigid body holding the point
    pub rigid_body: SceneRigidBody,
    /// The point in scene coordinates
    pub point: ScenePosition,
    /// Change of tether length per tile the point moves, such as the sum of the unit directions
    /// away from its neighbouring tether points
    pub gradient: [f32; 2],
}

/// A one-sided length limit between a pawn and the points a rope holds or bends around, solved
/// by the pawn's next movement step after its control and gravity
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct ScenePawnTether<'a> {
    /// Unit direction from the pawn toward the tether's nearest point
    pub direction: [f32; 2],
    /// Tether length beyond its allowed length in tiles, positive while overstretched
    pub stretch: f32,
    /// Rigid body points the tether holds, receiving the reaction of its pull
    pub bodies: &'a [ScenePawnTetherBody],
    /// Fraction of the pawn's speed across the tether lost per second while it pulls
    pub damping: f32,
}

/// Object hit by a scene ray
#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq)]
pub enum SceneRayTarget {
    /// Solid cellular tile cell
    CellularTile { cell: CellCoordinates },
    /// Cellular rigid body
    RigidBody { rigid_body: SceneRigidBody },
    /// Actor collider
    Actor { actor: Actor },
}

/// Closest object hit by a scene ray
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct SceneRayHit {
    /// Hit position in scene coordinates
    pub position: ScenePosition,
    /// Surface normal pointing out of the hit collider
    pub normal: [f32; 2],
    /// Distance from the cast origin in tiles
    pub distance: f64,
    /// Object that was hit
    pub target: SceneRayTarget,
}

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
    /// GPU simulation of the reactions in the active cellular tiles, absent until simulated or
    /// without reactions
    chemistry: Option<Chemistry>,
    /// Uploaded backgrounds of resident chunks
    backgrounds: SceneBackgrounds,
    /// Art of the materials that show a surface where they meet open space
    material_surfaces: Option<SceneMaterialSurfaces>,
    /// Whether material edges are drawn
    material_edges: bool,
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
    /// Linear light added to every lit appearance
    ambient_light: [f32; 3],
    /// Divisor of matter's extinction while light spreads into occluding matter
    light_penetration: f32,
    /// What is sensed around actors from the next simulation step
    actor_sensing_configuration: SceneActorSensingConfiguration,
    /// GPU sensing of the cells around actors, absent until simulated
    actor_sensing_pass: Option<ActorSensingPass>,
    /// Newest completed sensing of each sensed actor
    actor_sensing: HashMap<Actor, SceneActorSensing>,
    /// Soft bodies and the actors whose lives they share
    actor_soft_bodies: HashMap<SceneSoftBody, Actor>,
    /// GPU sampling of the cells around soft body particles, absent until simulated
    soft_body_sampling_pass: Option<SoftBodySamplingPass>,
    /// Simulation steps so far, the clock of decoration motion
    simulated_ticks: u64,
    /// Resolution used to calculate radiance cascade lighting
    lighting_quality: LightingQuality,
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
            chemistry: None,
            backgrounds: SceneBackgrounds::new(),
            material_surfaces: None,
            material_edges: false,
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
            ambient_light: [1.0, 1.0, 1.0],
            light_penetration: 8.0,
            actor_sensing_configuration: SceneActorSensingConfiguration::default(),
            actor_sensing_pass: None,
            actor_sensing: HashMap::new(),
            actor_soft_bodies: HashMap::new(),
            soft_body_sampling_pass: None,
            simulated_ticks: 0,
            lighting_quality: LightingQuality::High,
            actor_registry: ActorRegistry::new(),
            material_table: Arc::new(material_table),
            generator,
            world_seed,
        }
    }

    /// Returns the actors of this scene for modification
    pub fn actor_registry_mutable(&mut self) -> &mut ActorRegistry { &mut self.actor_registry }

    /// Sets what is sensed around actors from the next simulation step; unknown tags count
    /// nothing
    pub fn set_actor_sensing(
        &mut self,
        configuration: SceneActorSensingConfiguration,
    ) -> Result<(), SceneError> {
        if !is_valid_configuration(&configuration) {
            return Err(SceneError::InvalidActorSensingConfiguration);
        }
        self.actor_sensing_configuration = configuration;
        Ok(())
    }

    /// Returns the newest completed sensing of an actor, one or more simulation steps old, or
    /// `None` before its first readback or for an actor without a cellular tile actor record
    pub fn actor_sensing(&self, actor: Actor) -> Option<SceneActorSensing> {
        self.actor_sensing.get(&actor).copied()
    }

    /// Returns the edges of every collider in the collision world as line segments for debugging
    pub fn collider_outlines(&self) -> Vec<(ScenePosition, ScenePosition)> {
        self.collision.as_ref().map_or_else(Vec::new, Collision::collider_outlines)
    }

    /// Casts a ray from a scene position along a direction for up to a distance in tiles, returning
    /// the nearest hit among colliders in the filter's collision groups, ignoring an excluded actor
    pub fn raycast(
        &self,
        origin: ScenePosition,
        direction: [f64; 2],
        maximum_distance: f64,
        filter: u32,
        exclude: Option<Actor>,
    ) -> Option<SceneRayHit> {
        self.collision.as_ref()?.raycast(origin, direction, maximum_distance, filter, exclude)
    }

    /// Converts a scene position to a live cellular rigid body's local frame, in tiles
    pub fn rigid_body_local_point(
        &self,
        rigid_body: SceneRigidBody,
        position: ScenePosition,
    ) -> Option<[f32; 2]> {
        self.collision.as_ref()?.rigid_body_local_point(rigid_body, position)
    }

    /// Converts a point in a live cellular rigid body's local frame, in tiles, to a scene position
    pub fn rigid_body_scene_position(
        &self,
        rigid_body: SceneRigidBody,
        local: [f32; 2],
    ) -> Option<ScenePosition> {
        self.collision.as_ref()?.rigid_body_scene_position(rigid_body, local)
    }

    /// Returns a pawn's velocity in tiles per second
    pub fn pawn_velocity(&self, actor: Actor) -> Option<[f32; 2]> {
        self.collision.as_ref()?.pawn_velocity(actor)
    }

    /// Returns the largest speed, in tiles per second, at which a pawn struck a solid surface during
    /// its last movement step: its velocity relative to the surface along the surface normal, taken
    /// before the hit stopped it. Returns zero without a hit or `None` for an actor that is not a
    /// simulated pawn
    pub fn pawn_impact_speed(&self, actor: Actor) -> Option<f32> {
        self.collision.as_ref()?.pawn_impact_speed(actor)
    }

    /// Returns whether a pawn was grounded at its last movement step
    pub fn pawn_is_grounded(&self, actor: Actor) -> Option<bool> {
        self.collision.as_ref()?.pawn_is_grounded(actor)
    }

    /// Sets or clears the tether solved by a pawn's next movement step, returning whether the
    /// actor is a simulated pawn; a tether lasts one simulation step and is set again each update
    pub fn set_pawn_tether(&mut self, actor: Actor, tether: Option<ScenePawnTether<'_>>) -> bool {
        self.collision.as_mut().is_some_and(|collision| collision.set_pawn_tether(actor, tether))
    }

    /// Returns how many static cells a soft body holds
    pub fn soft_body_cell_count(&self, body: SceneSoftBody) -> Option<usize> {
        let record = self.collision.as_ref()?.soft_bodies.bodies.get(&body)?;
        Some(record.segments.iter().map(|segment| segment.cell_count()).sum())
    }

    /// Creates a soft body, or returns `None` before the scene first simulates or beyond the
    /// capacity for cells carried by moving bodies
    pub fn spawn_soft_body(&mut self, descriptor: &SoftBodyDescriptor) -> Option<SceneSoftBody> {
        self.collision.as_mut()?.spawn_soft_body(descriptor)
    }

    /// Rebuilds a soft body of no cells from a descriptor, keeping its identifier; it continues from
    /// the descriptor's `initial_state`, so its undeformed shape can change while it stays where it is
    pub fn reshape_soft_body(&mut self, body: SceneSoftBody, descriptor: &SoftBodyDescriptor) -> bool {
        self.collision.as_mut().is_some_and(|collision| collision.reshape_soft_body(body, descriptor))
    }

    /// Cuts a soft body along a line between two positions: its particles split along the line,
    /// opening a slit, and if that parts the body it comes apart, reported by `take_soft_body_events`
    pub fn cut_soft_body(
        &mut self,
        body: SceneSoftBody,
        start: ScenePosition,
        end: ScenePosition,
    ) -> bool {
        self.collision.as_mut().is_some_and(|collision| collision.cut_soft_body(body, start, end))
    }

    /// Returns a soft body's particles and how they are joined now
    pub fn soft_body_particles(&self, body: SceneSoftBody) -> Option<SoftBodyParticles> {
        self.collision.as_ref()?.soft_bodies.particles(body)
    }

    /// Adds velocity in tiles per second to a pawn at its next movement step, returning whether the
    /// actor is a simulated pawn
    pub fn add_pawn_velocity(&mut self, actor: Actor, velocity: [f32; 2]) -> bool {
        self.collision.as_mut().is_some_and(|collision| collision.add_pawn_velocity(actor, velocity))
    }

    /// Adds velocity like `add_pawn_velocity`, also taking away most of the pawn's footing for a
    /// moment so that its own control does not cancel the push
    pub fn shove_pawn(&mut self, actor: Actor, velocity: [f32; 2]) -> bool {
        self.collision.as_mut().is_some_and(|collision| collision.shove_pawn(actor, velocity))
    }

    /// Returns the solid static cells of the simulated region as the CPU knows them
    pub fn terrain(&self) -> Option<SceneTerrain<'_>> {
        self.collision.as_ref()?.terrain()
    }

    /// Removes a soft body, returning whether it existed
    pub fn remove_soft_body(&mut self, body: SceneSoftBody) -> bool {
        self.collision.as_mut().is_some_and(|collision| collision.remove_soft_body(body))
    }

    /// Tears the edge joining two neighboring particles of a soft body, returning whether it existed;
    /// the pieces are reported by `take_soft_body_events`
    pub fn tear_soft_body(&mut self, body: SceneSoftBody, first: u32, second: u32) -> bool {
        self.collision.as_mut().is_some_and(|collision| collision.tear_soft_body(body, first, second))
    }

    /// Takes what happened to soft bodies since the last call
    pub fn take_soft_body_events(&mut self) -> Vec<SoftBodyEvent> {
        self.collision.as_mut().map_or_else(Vec::new, |collision| collision.soft_bodies.take_events())
    }

    /// Returns the identifiers of the live soft bodies
    pub fn soft_bodies(&self) -> Vec<SceneSoftBody> {
        self.collision.as_ref().map_or_else(Vec::new, |collision| collision.soft_bodies.identifiers())
    }

    /// Returns the number of particles of a soft body
    pub fn soft_body_particle_count(&self, body: SceneSoftBody) -> Option<usize> {
        self.collision.as_ref()?.soft_bodies.particle_count(body)
    }

    /// Returns a soft body particle's position
    pub fn soft_body_particle_position(
        &self,
        body: SceneSoftBody,
        particle: usize,
    ) -> Option<ScenePosition> {
        self.collision.as_ref()?.soft_bodies.particle_position(body, particle)
    }

    /// Returns a soft body particle's velocity in tiles per second
    pub fn soft_body_particle_velocity(
        &self,
        body: SceneSoftBody,
        particle: usize,
    ) -> Option<[f32; 2]> {
        self.collision.as_ref()?.soft_bodies.particle_velocity(body, particle)
    }

    /// Returns the rest length along a soft body's edges between two particles, or `None` if no
    /// edges join them
    pub fn soft_body_path_length(&self, body: SceneSoftBody, first: u32, second: u32) -> Option<f32> {
        self.collision.as_ref()?.soft_bodies.path_length(body, first, second)
    }

    /// Returns the newest sample of the cell grid around a soft body particle
    pub fn soft_body_particle_environment(
        &self,
        body: SceneSoftBody,
        particle: usize,
    ) -> Option<SoftBodyEnvironment> {
        self.collision.as_ref()?.soft_bodies.particle_environment(body, particle)
    }

    /// Returns the anchors still holding a soft body
    pub fn soft_body_anchors(&self, body: SceneSoftBody) -> Option<&[SoftBodyAnchor]> {
        self.collision.as_ref()?.soft_bodies.anchors(body)
    }

    /// Returns whether a soft body is asleep
    pub fn soft_body_is_sleeping(&self, body: SceneSoftBody) -> Option<bool> {
        self.collision.as_ref()?.soft_bodies.is_sleeping(body)
    }

    /// Returns the particle nearest a position within a distance in tiles, with its distance
    pub fn nearest_soft_body_particle(
        &self,
        position: ScenePosition,
        distance: f32,
    ) -> Option<(SceneSoftBody, usize, f32)> {
        self.collision.as_ref()?.soft_bodies.nearest_particle(position, distance)
    }

    /// Holds a soft body particle so that it follows a position, or lets it go with `None`
    pub fn set_soft_body_particle_target(
        &mut self,
        body: SceneSoftBody,
        particle: usize,
        target: Option<ScenePosition>,
    ) {
        if let Some(collision) = self.collision.as_mut() {
            collision.soft_bodies.set_particle_target(body, particle, target);
        }
    }

    /// Sets whether a soft body particle passes through solid cells, for one caught on them
    pub fn set_soft_body_particle_passes_terrain(
        &mut self,
        body: SceneSoftBody,
        particle: usize,
        passes: bool,
    ) {
        if let Some(collision) = self.collision.as_mut() {
            collision.soft_bodies.set_particle_passes_terrain(body, particle, passes);
        }
    }

    /// Adds velocity in tiles per second to a soft body particle and wakes its body
    pub fn add_soft_body_particle_velocity(
        &mut self,
        body: SceneSoftBody,
        particle: usize,
        velocity: [f32; 2],
    ) {
        if let Some(collision) = self.collision.as_mut() {
            collision.soft_bodies.add_particle_velocity(body, particle, velocity);
        }
    }

    /// Pushes every soft body particle near a position outward, falling off linearly to zero at a
    /// radius in tiles, in tiles per second at the center
    pub fn add_soft_body_radial_velocity(
        &mut self,
        center: ScenePosition,
        radius: f32,
        speed: f32,
    ) {
        if let Some(collision) = self.collision.as_mut() {
            collision.soft_bodies.add_radial_velocity(center, radius, speed);
        }
    }

    /// Sets the material tags reported in soft body particle environments, at most eight
    pub fn set_soft_body_environment_tags(&mut self, tags: Vec<String>) {
        if let Some(collision) = self.collision.as_mut() {
            collision.soft_bodies.set_environment_tags(tags);
        }
    }

    /// Returns the downward acceleration of dynamic bodies and pawns in tiles per second squared
    pub fn gravity(&self) -> f32 { self.gravity }

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

    /// Returns the linear light added to every lit appearance
    pub fn ambient_light(&self) -> [f32; 3] { self.ambient_light }

    /// Sets the linear light added to every lit appearance, changeable each frame; `[1.0, 1.0,
    /// 1.0]` shows albedo unchanged where nothing else lights it
    pub fn set_ambient_light(&mut self, color: [f32; 3]) -> Result<(), SceneError> {
        if color.iter().any(|channel: &f32| !channel.is_finite() || *channel < 0.0) {
            return Err(SceneError::InvalidAmbientLight);
        }
        self.ambient_light = color;
        Ok(())
    }

    /// Returns the radiance cascade lighting resolution
    pub fn lighting_quality(&self) -> LightingQuality { self.lighting_quality }

    /// Sets the radiance cascade lighting resolution
    pub fn set_lighting_quality(&mut self, quality: LightingQuality) {
        self.lighting_quality = quality;
    }

    /// Returns the divisor of matter's extinction while light spreads into occluding matter
    pub fn light_penetration(&self) -> f32 { self.light_penetration }

    /// Sets the divisor of matter's extinction while light spreads into occluding matter from its
    /// lit surface; one spreads no deeper than direct light, larger values let light soak further
    /// into solid matter
    pub fn set_light_penetration(&mut self, penetration: f32) -> Result<(), SceneError> {
        if !penetration.is_finite() || penetration < 1.0 {
            return Err(SceneError::InvalidLightPenetration);
        }
        self.light_penetration = penetration;
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

    /// Sets the sprite sheet of the sprites in chunk backgrounds
    pub fn set_background_sprites(&mut self, sprite_sheet: Arc<SpriteSheet>) {
        self.backgrounds.set_sprite_sheet(sprite_sheet);
    }

    /// Sets the art shown where materials meet open space; other materials are drawn unchanged
    pub fn set_material_surfaces(
        &mut self,
        accelerator: &Accelerator,
        surfaces: &[MaterialSurface<'_>],
    ) {
        self.material_surfaces =
            Some(SceneMaterialSurfaces::new(accelerator, surfaces, &self.material_table));
    }

    /// Sets whether material edges are drawn: surface art with its color shift, and the shadows
    /// solid cells cast on backgrounds behind them; off by default, which also skips their
    /// computation
    pub fn set_material_edges(&mut self, material_edges: bool) {
        self.material_edges = material_edges;
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
        self.backgrounds.stream(accelerator, chunk_streaming_state, camera_position);
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
                &self.material_table,
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
        Self::simulate_actor_soft_bodies(
            collision,
            &mut self.actor_registry,
            &mut self.actor_soft_bodies,
            active_region,
        );
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
                self.material_table.has_fluid_reaction_products(),
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
        // heat moves through every cell after the matter carrying it has moved, and the matter
        // of the tiles it lists reacts first, so the reactions' heat and products take part
        let thermal: &mut Thermal = self.thermal.as_mut()
            .expect("thermal simulation exists before fluid conversions");
        if self.material_table.has_reactions() {
            let fluids: &Fluids = self.fluids.as_ref()
                .expect("fluid storage exists with the tile buffers");
            self.chemistry
                .get_or_insert_with(|| Chemistry::new(
                    accelerator,
                    cellular_tile_buffers,
                    &self.material_table,
                    fluids.fluid_species_buffer(),
                    fluids.fluid_heat_buffer(),
                    fluids.fluid_spawn_buffer(),
                    collision.rigid_body_pass().cellular_tile_buffer(),
                    collision.changed_rigid_body_cell_buffer(),
                    thermal.control_buffer(),
                ))
                .prepare_chemistry(
                    accelerator,
                    active_region,
                    cellular_tile_streaming_state.cellular_tile_ring_offset(),
                    ambient_gas_amount,
                    self.ambient_gas_heat_capacity,
                );
        }
        thermal.simulate_thermal(
            accelerator,
            active_region,
            cellular_tile_streaming_state.cellular_tile_ring_offset(),
            self.ambient_gas_temperature,
            self.ambient_gas_heat_capacity,
            self.ambient_gas_conduction,
            self.chemistry.as_ref(),
        );
        let fluids: &mut Fluids = self.fluids.as_mut()
            .expect("fluid storage exists with the tile buffers");
        fluids.simulate_heat(accelerator);
        collision.request_changed_cells(
            accelerator,
            active_region,
            cellular_tile_streaming_state.cellular_tile_ring_offset(),
        );
        // actors sense the cells as every simulation left them this step; the results are read
        // back without waiting at a later step, so sensing never stalls on the GPU
        let cellular_tiles: &CellularTiles = self.cellular_tiles
            .as_ref()
            .expect("cellular tile simulation exists after simulation");
        let actor_sensing_pass: &mut ActorSensingPass = self.actor_sensing_pass
            .get_or_insert_with(|| ActorSensingPass::new(
                accelerator,
                cellular_tile_buffers,
                &self.material_table,
                cellular_tiles.actor_buffer(),
                fluids.fluid_raster_buffer(),
            ));
        actor_sensing_pass.read(accelerator, &mut self.actor_sensing)?;
        actor_sensing_pass.configure(
            accelerator,
            &self.material_table,
            &self.actor_sensing_configuration,
        );
        let sensed_actor_records: Vec<Actor> = cellular_tiles.fluid_actors().iter()
            .map(|(actor, _)| *actor)
            .collect();
        actor_sensing_pass.sense(
            accelerator,
            active_region,
            cellular_tile_streaming_state.cellular_tile_ring_offset(),
            &sensed_actor_records,
        );
        self.simulated_ticks += 1;
        // soft bodies sample the same finished cells, and read earlier samples without waiting
        let fluids: &Fluids = self.fluids.as_ref()
            .expect("fluid storage exists with the tile buffers");
        let collision: &mut Collision = self.collision.as_mut()
            .expect("collision simulation exists after simulation");
        if !collision.soft_bodies.bodies.is_empty() {
            let sampling_pass: &mut SoftBodySamplingPass = self.soft_body_sampling_pass
                .get_or_insert_with(|| SoftBodySamplingPass::new(
                    accelerator,
                    cellular_tile_buffers,
                    &self.material_table,
                    fluids.fluid_raster_buffer(),
                ));
            sampling_pass.configure(accelerator, &self.material_table, &mut collision.soft_bodies);
            sampling_pass.read(accelerator, &mut collision.soft_bodies, self.gravity)?;
            sampling_pass.sample(
                accelerator,
                active_region,
                cellular_tile_streaming_state.cellular_tile_ring_offset(),
                &collision.soft_bodies,
            );
        }
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
                radiance_sheet,
                frame,
                offset_x,
                offset_y,
                scale_x,
                scale_y,
                rotation,
                color,
                radiance_intensity,
                depth,
            ) in &actor_sprites.sprites {
                if !radiance_intensity.is_finite() || *radiance_intensity < 0.0 { continue; }
                let Some(texture_region): Option<[f32; 4]> =
                    sprite_sheet.frame_texture_region(*frame)
                else { continue; };
                let radiance_frame: u32 = frame % radiance_sheet.frame_count();
                let Some(radiance_texture_region): Option<[f32; 4]> =
                    radiance_sheet.frame_texture_region(radiance_frame)
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
                        radiance_texture: radiance_sheet.texture_view(),
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
                        radiance_texture_region,
                        radiance_intensity: *radiance_intensity,
                    });
                    continue;
                }
                sprites.sprites.push((
                    sprite_sheet.texture_view(),
                    radiance_sheet.texture_view(),
                    (actor_x + f64::from(*offset_x)) as f32,
                    (actor_y + f64::from(*offset_y)) as f32,
                    sprite_sheet.width() as f32 * *scale_x / CELLULAR_TILE_SIZE as f32,
                    sprite_sheet.height() as f32 * *scale_y / CELLULAR_TILE_SIZE as f32,
                    *rotation,
                    *depth,
                    texture_region,
                    radiance_texture_region,
                    *color,
                    *radiance_intensity,
                ));
            }
        }
        Ok(SceneGraphics {
            shaded: self.shaded_bodies(camera_position),
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
            temperature: self.temperature_graphics(camera_position),
            parallax,
            parallax_batches: self.backgrounds.batches(
                camera_position,
                camera.visible_width(),
                camera.visible_height(),
            ),
            decorations: self.backgrounds.decoration_atlas().map(|atlas| {
                SceneGraphicsDecorations {
                    atlas,
                    batches: self.backgrounds.decoration_batches(
                        camera_position,
                        camera.visible_width(),
                        camera.visible_height(),
                    ),
                    glowing: self.backgrounds.glowing_decoration_batches(camera_position),
                    movers: self.decoration_movers(camera_position),
                    time: self.simulated_ticks as f32 / 60.0,
                }
            }),
            material_surfaces: self.material_surfaces.as_ref()
                .map(SceneMaterialSurfaces::graphics),
            material_edges: self.material_edges,
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
            lighting: SceneGraphicsLighting {
                ambient: self.ambient_light,
                penetration: self.light_penetration,
                quality: self.lighting_quality,
                temperature_color_buffer: self.material_table.temperature_color_buffer(),
                amount_color_buffer: self.material_table.amount_color_buffer(),
                thermal_offsets: self.material_table.thermal_offsets(),
                // lights with nonfinite or negative values are skipped
                lights: self.actor_registry
                    .components::<ActorLightComponent, ActorKinematicsPositionComponent>()
                    .filter(|(light, _)| {
                        light.color.iter().chain([&light.intensity, &light.radius])
                            .all(|value| value.is_finite() && *value >= 0.0)
                    })
                    .map(|(light, position)| {
                        let (center_x, center_y): (f64, f64) =
                            position.position.relative_position(camera_position);
                        SceneGraphicsLight {
                            center: [center_x as f32, center_y as f32],
                            radius: light.radius,
                            emission: light.color.map(|channel| channel * light.intensity),
                        }
                    })
                    .collect(),
            },
        })
    }

    /// Keeps soft bodies and the actors that own them together: creates what the actors asked for,
    /// applies their pushes, moves them with their bodies, despawns those whose bodies reached the
    /// edge of the region, and removes the bodies of actors that are gone
    fn simulate_actor_soft_bodies(
        collision: &mut Collision,
        actor_registry: &mut ActorRegistry,
        actor_soft_bodies: &mut HashMap<SceneSoftBody, Actor>,
        active_region: CellularTileRegion,
    ) {
        let actors: Vec<Actor> = actor_registry
            .components::<ActorIdentityComponent, ActorSoftBodyComponent>()
            .map(|(identity, _)| identity.actor())
            .collect();
        let origin: CellularTileCoordinates = collision.origin();
        let minimum: [f32; 2] = [
            (active_region.bottom_left.x - origin.x) as f32 + ACTOR_SOFT_BODY_REGION_MARGIN,
            (active_region.bottom_left.y - origin.y) as f32 + ACTOR_SOFT_BODY_REGION_MARGIN,
        ];
        let maximum: [f32; 2] = [
            (active_region.top_right.x - origin.x + 1) as f32 - ACTOR_SOFT_BODY_REGION_MARGIN,
            (active_region.top_right.y - origin.y + 1) as f32 - ACTOR_SOFT_BODY_REGION_MARGIN,
        ];
        for actor in actors {
            let is_persistent: bool = actor_registry.component::<ActorPersistenceComponent>(actor).is_ok();
            let Ok(component) = actor_registry.component_mutable::<ActorSoftBodyComponent>(actor)
            else { continue; };
            if component.body.is_none() {
                if let Some(descriptor) = component.descriptor.as_ref() {
                    match collision.spawn_soft_body(descriptor) {
                        Some(body) => {
                            component.body = Some(body);
                            component.descriptor = None;
                            component.is_unplaced = false;
                        }
                        None => component.is_unplaced = true,
                    }
                }
            }
            let Some(body) = component.body else { continue; };
            actor_soft_bodies.insert(body, actor);
            let impulses: Vec<ActorSoftBodyImpulse> = std::mem::take(&mut component.impulses);
            let Some(record) = collision.soft_bodies.bodies.get(&body) else { continue; };
            let (low, high): ([f32; 2], [f32; 2]) = (record.bounds_minimum, record.bounds_maximum);
            let count: u32 = record.settled.len() as u32;
            for impulse in impulses {
                match impulse {
                    ActorSoftBodyImpulse::Particle { particle, velocity } =>
                        collision.soft_bodies.add_particle_velocity(body, particle as usize, velocity),
                    ActorSoftBodyImpulse::All { velocity } => for particle in 0..count {
                        collision.soft_bodies.add_particle_velocity(body, particle as usize, velocity);
                    },
                }
            }
            let center: ScenePosition = collision.soft_bodies
                .scene_position([(low[0] + high[0]) * 0.5, (low[1] + high[1]) * 0.5]);
            if let Ok(position) = actor_registry
                .component_mutable::<ActorKinematicsPositionComponent>(actor)
            {
                position.position = center;
            }
            // what lies beyond the region is not there to stand on
            let is_leaving: bool = low[0] < minimum[0] || low[1] < minimum[1]
                || high[0] > maximum[0] || high[1] > maximum[1];
            if is_leaving && !is_persistent { actor_registry.despawn(actor).ok(); }
        }
        actor_soft_bodies.retain(|body, actor| {
            let is_owned: bool = actor_registry.component::<ActorSoftBodyComponent>(*actor)
                .is_ok_and(|component| component.body == Some(*body));
            if !is_owned { collision.remove_soft_body(*body); }
            is_owned
        });
    }

    /// Returns the outlines of the soft bodies of actors with shaders, relative to a camera
    fn shaded_bodies(&self, camera_position: ScenePosition) -> Vec<SceneGraphicsShaded<'_>> {
        let Some(collision) = self.collision.as_ref() else { return Vec::new(); };
        let mut shaded: Vec<SceneGraphicsShaded<'_>> = Vec::new();
        for (component, shader) in self.actor_registry
            .components::<ActorSoftBodyComponent, ActorShaderComponent>()
        {
            let Some(body) = component.body else { continue; };
            let Some(particles) = collision.soft_bodies.particles(body) else { continue; };
            // the outline runs particle to particle, its longest loop being what is seen
            let next: HashMap<u32, u32> = particles.outline.iter().map(|side| (side[0], side[1])).collect();
            let mut visited: HashSet<u32> = HashSet::new();
            let mut best: Vec<u32> = Vec::new();
            for side in &particles.outline {
                if visited.contains(&side[0]) { continue; }
                let mut chain: Vec<u32> = Vec::new();
                let mut at: u32 = side[0];
                while visited.insert(at) {
                    chain.push(at);
                    let Some(following) = next.get(&at) else { break; };
                    at = *following;
                }
                if chain.len() > best.len() { best = chain; }
            }
            if best.len() < 3 { continue; }
            let offset: [f64; 2] = {
                let (x, y): (f64, f64) = particles.origin.relative_position(camera_position);
                [x, y]
            };
            let points: Vec<[f32; 2]> = best.iter().filter_map(|particle| {
                let position: [f32; 2] = *particles.positions.get(*particle as usize)?;
                Some([
                    (offset[0] + f64::from(position[0])) as f32,
                    (offset[1] + f64::from(position[1])) as f32,
                ])
            }).collect();
            // a smooth curve through the particles, so that no corner of the mesh ever shows
            let mut outline: Vec<[f32; 2]> = Vec::with_capacity(points.len() * ACTOR_SHADER_CURVE_STEPS);
            let count: usize = points.len();
            for index in 0..count {
                let (a, b, c, d): ([f32; 2], [f32; 2], [f32; 2], [f32; 2]) = (
                    points[(index + count - 1) % count],
                    points[index],
                    points[(index + 1) % count],
                    points[(index + 2) % count],
                );
                for step in 0..ACTOR_SHADER_CURVE_STEPS {
                    let t: f32 = step as f32 / ACTOR_SHADER_CURVE_STEPS as f32;
                    let curve = |a: f32, b: f32, c: f32, d: f32| {
                        0.5 * (2.0 * b + (c - a) * t + (2.0 * a - 5.0 * b + 4.0 * c - d) * t * t
                            + (3.0 * b - a - 3.0 * c + d) * t * t * t)
                    };
                    outline.push([curve(a[0], b[0], c[0], d[0]), curve(a[1], b[1], c[1], d[1])]);
                }
            }
            shaded.push(SceneGraphicsShaded {
                fragment: shader.fragment,
                parameters: shader.parameters,
                is_cell_snapped: shader.is_cell_snapped,
                outline,
            });
        }
        shaded
    }

    /// Returns the bounds and velocities of the actors nearest a camera, which push the motion field
    /// that decorations read, in tiles and tiles per second relative to it
    fn decoration_movers(&self, camera_position: ScenePosition) -> [SceneGraphicsDecorationMover; 8] {
        let mut movers: [SceneGraphicsDecorationMover; 8] = Default::default();
        let Some(collision) = self.collision.as_ref() else { return movers; };
        let mut nearest: Vec<(f32, SceneGraphicsDecorationMover)> = collision.actors()
            .filter_map(|actor| {
                let position: ScenePosition = self.actor_registry
                    .component::<ActorKinematicsPositionComponent>(actor).ok()?.position;
                let collider: &Collider = &self.actor_registry
                    .component::<ActorCollisionComponent>(actor).ok()?.collider;
                let (x, y): (f64, f64) = position.relative_position(camera_position);
                let half_size: [f32; 2] = match collider.shape {
                    ColliderShape::Circle { radius } => [radius, radius],
                    ColliderShape::Rectangle { width, height }
                    | ColliderShape::Capsule { width, height } => [width * 0.5, height * 0.5],
                };
                let position: [f32; 2] = [x as f32 + collider.offset_x, y as f32 + collider.offset_y];
                Some((position[0] * position[0] + position[1] * position[1], SceneGraphicsDecorationMover {
                    position,
                    half_size,
                    velocity: collision.pawn_velocity(actor).unwrap_or([0.0, 0.0]),
                }))
            })
            .collect();
        nearest.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (mover, (_, record)) in movers.iter_mut().zip(nearest) { *mover = record; }
        movers
    }

    /// Sets the atlas of the decorations in chunk backgrounds
    pub fn set_decoration_atlas(&mut self, atlas: Arc<SpriteSheet>) {
        self.backgrounds.set_decoration_atlas(atlas);
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
            region_tile_x: active_region.bottom_left.x,
            region_tile_y: active_region.bottom_left.y,
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
        Some(SceneGraphicsFluid {
            raster_buffer: fluids.fluid_raster_buffer(),
            color_buffer: fluids.fluid_color_buffer(),
            heat_buffer: fluids.fluid_heat_buffer(),
            species_buffer: fluids.fluid_species_buffer(),
            material_properties_buffer: self.material_table.fluid_properties_buffer(),
            material_graphics_buffer: self.material_table.fluid_graphics_buffer(),
            material_identifier_buffer: cellular_tile_buffers.material_identifier_buffer(),
            region_x: region_x as f32,
            region_y: region_y as f32,
            region_width: active_region.width() as u32,
            region_height: active_region.height() as u32,
            ring_offset_x,
            ring_offset_y,
            cellular_tile_size: CELLULAR_TILE_SIZE as u32,
            time: self.simulated_ticks as f32 / 60.0,
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
            material_graphics_buffer: self.material_table.gas_graphics_buffer(),
            region_x: region_x as f32,
            region_y: region_y as f32,
            region_width: active_region.width() as u32,
            region_height: active_region.height() as u32,
            ring_offset_x,
            ring_offset_y,
            cellular_tile_size: CELLULAR_TILE_SIZE as u32,
            amount_per_atmosphere: GAS_AMOUNT_PER_ATMOSPHERE,
            time: self.simulated_ticks as f32 / 60.0,
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

    /// Returns the resident cell temperatures relative to a camera, or `None` before they stream
    pub fn temperature_graphics(
        &self,
        camera_position: ScenePosition,
    ) -> Option<SceneGraphicsTemperature<'_>> {
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
        Some(SceneGraphicsTemperature {
            matter_buffer: cellular_tile_buffers.matter_buffer(),
            ambient_temperature: self.ambient_gas_temperature,
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
                &self.material_table,
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
