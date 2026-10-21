+++
title = "Cell and body contact exchange"
description = "How the GPU cellular simulation reports actor and rigid-body contacts to collision resolution."
weight = 80
[extra]
sources = ["engine_world/src/scenes/fluids", "engine_world/src/scenes/kinematics/kinematics.rs", "engine_world/src/scenes/kinematics/kinematics_pass.rs", "engine_world/src/scenes/kinematics/kinematics_cellular_tile.wgsl", "engine_world/src/scenes/kinematics/mod.rs", "engine_world/src/scenes/collisions/collision.rs", "engine_world/src/scenes/collisions/mod.rs", "engine_world/src/scenes/scene.rs"]
+++
This page documents the records kinematics sends to and receives from collision. It is for engine contributors working on actor and rigid-body contact response.

## Inputs to cellular kinematics

The scene runs kinematics before Rapier collision simulation. Kinematics uploads 64-byte actor records and shares each rigid body's 64-byte record with the rasterization pass (selection rules and caps: [cellular kinematics](@/internals/simulation/cellular-kinematics.md#actors-and-cellular-rigid-bodies); layouts: [kinematics shader](@/internals/shaders/kinematics-shader.md)).

The pass also receives `(GPU record index, collision identifier)` pairs for cellular rigid bodies, which it uses to find the body records shared with the collision rasterization pass.

## Contact results

The result maps have these tuple shapes:

| Map | Tuple fields |
| --- | --- |
| `cellular_contacts: HashMap<Actor, ([f32; 2], [f32; 2], f32)>` | `(velocity change in tiles/second, contact normal, penetration depth in tiles)` |
| `rigid_body_contacts: HashMap<u64, ([f32; 2], f32, [[f32; 5]; 2], [f32; 7])>` | `(linear impulse in mass·tiles/second, angular impulse in mass·tiles²/second, two holding-contact records, seven wet-face fluid moments)` |

### Actor contacts

The GPU accumulates impulse components, summed contact-normal components, contact count, and maximum penetration depth. Normals point from overlapping dynamic cells toward the actor interior. Rust then:

- converts the impulse from mass·cells/tick to velocity change in tiles/second (divide by actor mass, apply the cell-to-tile and tick-to-second scales);
- normalizes the summed normal;
- converts depth from cells to tiles.

A map entry is omitted if both velocity change and normal are zero.

### Rigid-body contacts

The GPU accumulates x/y impulse and angular impulse per body. Rust scales the linear values by tiles per second and the angular value by the cell-to-tile scale and tiles per second.

Each of the two five-float holding records contains contact count, summed normal x/y, and summed arm x/y. Each arm is measured from the body's center of mass and converted to tiles. Contacts are split between the records by the sign of the arm's projection onto the body's local x axis.

Collision normalizes the summed normal and divides the summed arm by count, keeping a normal and average arm per side. Holding contacts persist until a newer readback arrives.

## Readback timing and collision response

Contact buffers are copied to one of three staging slots per contact type and mapped asynchronously. A kinematics call returns completed readbacks, if any, rather than waiting for the current GPU tick. The contacts therefore describe an earlier tick and can arrive later than one tick if readback is still in flight.

- Actor readbacks are drained in submission order. If several are ready, only the newest map is returned and earlier actor deltas are discarded.
- Kinematics then zeros the returned velocity changes in its cache, retaining normals and depths until another map arrives.
- Rigid-body readbacks accumulate impulses by body identifier.

> **Limitation:** If all three actor-contact readback slots are in flight, that tick's actor contacts are skipped and cleared on the next simulation call. Rigid-body contacts remain accumulated until a slot is available.

In the following collision update, collision applies each rigid body's reported linear and angular impulses, updates its holding contacts, and runs the Rapier step.

Before the Rapier step, the holding solver runs four iterations for each awake rigid body; sleeping bodies are skipped. It applies impulses that cancel motion into the dynamic cells holding the body back, including the velocity gravity adds during the step. See [cellular kinematics](@/internals/simulation/cellular-kinematics.md) for the GPU cell interactions and [Rapier collision world](@/internals/simulation/collision-world.md) for collision ordering.

Fluid actor feedback uses separate asynchronous 32-byte records; [fluid interactions](@/internals/simulation/fluid-coupling.md#actors) describes its cache and response. The [fluid shader reference](@/internals/shaders/fluid-shader.md#packed-records) owns the GPU layout.

### Rigid-body fluid moments

The final seven values are wet-face mass, x/y momentum, angular current moment, squared-arm mass moment, and x/y mass-weighted arm. Rust converts momentum from cells/tick to tiles/second, angular current by an additional cell-to-tile factor, and arm moments to tiles or tiles². Impulses accumulate over completed readbacks, but holding records are replaced and the seven fluid moments are summed. Collision uses them only when a result map entry arrives; it does not maintain a separate persistent fluid-moment cache. See [rigid-body coupling](@/internals/simulation/fluid-coupling.md#cellular-rigid-bodies).
