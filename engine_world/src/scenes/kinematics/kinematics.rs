// Copyright Rob Gage 2026

use super::{
    kinematics_pass::KinematicsPass,
    KINEMATICS_FIXED_MASS,
};
use crate::{
    actors::{
        components::{
            ActorCollisionComponent,
            ActorIdentityComponent,
            ActorKinematicsPositionComponent,
        },
        Actor,
        ActorRegistry,
    },
    materials::MaterialTable,
    scenes::{
        cellular_tiles::{
            CellularTileBuffers,
            CellularTileRegion,
        },
        collisions::{
            ColliderShape,
            CollisionBody,
            CollisionRigidBodyPass,
            COLLISION_GROUP_CELLULAR_TILE,
        },
        SceneError,
        ScenePosition,
    },
};
use engine_host::acceleration::Accelerator;
use std::collections::HashMap;
use std::collections::HashSet;

/// Distance in tiles beyond the active region within which actors still reach its cells
const KINEMATICS_ACTOR_REGION_MARGIN: f64 = 4.0;

/// GPU simulation of the dynamic cells of a scene's active cellular tiles and the actors in them
pub struct Kinematics {
    /// Position of every simulated actor in the previous tick, giving the velocity it pushes with
    actor_positions: HashMap<Actor, ScenePosition>,
    /// Newest contact normals and depths of every actor, kept while readbacks are in flight
    cellular_contacts: HashMap<Actor, ([f32; 2], [f32; 2], f32)>,
    /// Actor identities and collision settings aligned with the uploaded actor records
    fluid_actors: Vec<(Actor, ActorCollisionComponent)>,
    /// GPU simulation of dynamic cells and their fluid and body interactions
    pass: KinematicsPass,
}

impl Kinematics {

    /// Creates a simulation of the dynamic cells in the cellular tile buffers against the rigid
    /// bodies rasterized into them
    pub fn new(
        accelerator: &Accelerator,
        cellular_tile_buffers: &CellularTileBuffers,
        material_table: &MaterialTable,
        collision_rigid_body_pass: &CollisionRigidBodyPass,
        fluid_raster_buffer: &wgpu::Buffer,
    ) -> Self {
        Self {
            actor_positions: HashMap::new(),
            cellular_contacts: HashMap::new(),
            fluid_actors: Vec::new(),
            pass: KinematicsPass::new(
                accelerator,
                cellular_tile_buffers,
                material_table,
                collision_rigid_body_pass,
                fluid_raster_buffer,
            ),
        }
    }

    /// Returns the actor records shared with the fluid pass
    pub(crate) fn actor_buffer(&self) -> &wgpu::Buffer { self.pass.actor_buffer() }

    /// Returns actor identities and collision settings aligned with the shared GPU records
    pub(crate) fn fluid_actors(&self) -> &[(Actor, ActorCollisionComponent)] { &self.fluid_actors }

    /// Simulates one tick of the active dynamic cells against every actor colliding with cellular
    /// tiles and every rigid body at its record index, returning each actor's velocity change in
    /// tiles per second from the cells it pushed, and the normal and depth in tiles of the cells
    /// holding it back, read from an earlier tick, then each rigid body's impulse, angular impulse,
    /// and holding cells read since the last tick, with the region's bottom-left corner placed in
    /// cells from the rigid bodies' origin
    pub fn simulate_kinematics(
        &mut self,
        accelerator: &Accelerator,
        active_region: CellularTileRegion,
        ring_offset: (u32, u32),
        region_origin: (i32, i32),
        gravity: f32,
        actor_registry: &ActorRegistry,
        swimming_actors: &HashSet<Actor>,
        rigid_bodies: &[(u32, u64)],
    ) -> Result<(
        HashMap<Actor, ([f32; 2], [f32; 2], f32)>,
        HashMap<u64, ([f32; 2], f32, [[f32; 5]; 2], [f32; 7])>,
    ), SceneError> {
        let region_position: ScenePosition = ScenePosition {
            tile_x: active_region.bottom_left.x,
            tile_y: active_region.bottom_left.y,
            offset_x: 0,
            offset_y: 0,
        };
        let region_width: f64 = active_region.width() as f64;
        let region_height: f64 = active_region.height() as f64;
        // place every nearby actor colliding with cellular tiles relative to the region, moving
        // by its change in position since the previous tick
        let mut actors: Vec<(Actor, ColliderShape, [f32; 2], f32, [f32; 2], f32, bool)> =
            Vec::new();
        let mut fluid_actors: Vec<(Actor, ActorCollisionComponent)> = Vec::new();
        let mut actor_positions: HashMap<Actor, ScenePosition> = HashMap::new();
        for (actor_identity, position) in actor_registry
            .components::<ActorIdentityComponent, ActorKinematicsPositionComponent>()
        {
            let actor: Actor = actor_identity.actor();
            let Ok(collision) = actor_registry.component::<ActorCollisionComponent>(actor)
            else { continue; };
            if collision.filter & COLLISION_GROUP_CELLULAR_TILE == 0 { continue; }
            let position: ScenePosition = position.position;
            let (x, y): (f64, f64) = position.relative_position(region_position);
            if x < -KINEMATICS_ACTOR_REGION_MARGIN || y < -KINEMATICS_ACTOR_REGION_MARGIN
                || x > region_width + KINEMATICS_ACTOR_REGION_MARGIN
                || y > region_height + KINEMATICS_ACTOR_REGION_MARGIN
            { continue; }
            let (velocity_x, velocity_y): (f64, f64) = self.actor_positions.get(&actor)
                .map_or((0.0, 0.0), |previous| position.relative_position(*previous));
            actor_positions.insert(actor, position);
            let mass: f32 = match collision.body {
                CollisionBody::Fixed => KINEMATICS_FIXED_MASS,
                CollisionBody::Kinematic | CollisionBody::Dynamic => collision.mass,
            };
            if !mass.is_finite() || mass <= 0.0 {
                return Err(SceneError::InvalidCollider { actor });
            }
            actors.push((
                actor,
                collision.collider.shape,
                [x as f32 + collision.collider.offset_x, y as f32 + collision.collider.offset_y],
                collision.collider.rotation,
                [velocity_x as f32, velocity_y as f32],
                mass,
                swimming_actors.contains(&actor),
            ));
            fluid_actors.push((actor, collision.clone()));
        }
        fluid_actors.truncate(super::KINEMATICS_ACTOR_COUNT_MAXIMUM);
        self.fluid_actors = fluid_actors;
        self.actor_positions = actor_positions;
        let (contacts, rigid_body_contacts): (
            Option<HashMap<Actor, ([f32; 2], [f32; 2], f32)>>,
            Option<HashMap<u64, ([f32; 2], f32, [[f32; 5]; 2], [f32; 7])>>,
        ) = self.pass.simulate_cellular_tiles(
            accelerator,
            active_region,
            ring_offset,
            region_origin,
            gravity,
            &actors,
            rigid_bodies,
        )?;
        let rigid_body_contacts: HashMap<u64, ([f32; 2], f32, [[f32; 5]; 2], [f32; 7])> =
            rigid_body_contacts.unwrap_or_default();
        // velocity changes apply once, while normals and depths last until the next readback
        if let Some(contacts) = contacts {
            self.cellular_contacts = contacts;
            let contacts: HashMap<Actor, ([f32; 2], [f32; 2], f32)> =
                self.cellular_contacts.clone();
            for (velocity_change, _, _) in self.cellular_contacts.values_mut() {
                *velocity_change = [0.0; 2];
            }
            return Ok((contacts, rigid_body_contacts));
        }
        Ok((self.cellular_contacts.clone(), rigid_body_contacts))
    }

}
