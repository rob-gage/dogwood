# Physics And Collision

Gameplay configures actors and materials; Dogwood owns the simulation stages.
Game code normally does not touch Rapier, WGPU buffers, or shader resources.

## Static Terrain

Static cellular materials form terrain in the resident scene. They can carry
integrity, transmit pressure, fracture into debris, detach into rigid cellular
bodies, and provide the delayed collision view used by actors and Rapier.
Use scene edits for gameplay placement or destruction rather than trying to
edit a collision object directly.

## Dynamic Cellular Matter

Dynamic cellular materials move cell by cell under gravity and contact rules.
They are useful for sand and debris. Configure their material mass, friction,
restitution, and pressure transmission. The engine handles movement and
resident-region persistence.

## Fluids And Gases

Fluids are particle-based and can interact with solid proxies, actor swimming,
and rigid bodies. Configure density, viscosity, rest density, smoothing, and
contact response on the fluid material. Gases are species in a shared flow
field; configure density, diffusivity, extinction, dissipation, and
compressibility.

## Rigid Bodies

Rigid cellular bodies preserve material cells while moving through CPU physics.
Place one with `SceneEditBatch::place_rigid_body` or let detached terrain form
one when its component meets the material minimum-size rule. The body has a
Rapier transform/collider plus a derived cellular proxy for pressure, thermal,
fluid, and granular interaction.

## Actors And Shapes

Actors choose `Circle`, `Capsule`, or `Rectangle` collision geometry. Pawns can
enable walking, swimming, flying, or noclip; the active movement mode is set
by `ActorPawn::movement`. Walking configuration uses tiles per second for
`speed`, tiles per second squared for `acceleration`, tiles per second for
`jump_velocity`, and radians for `maximum_slope_angle`. `mass` is the effective
mass used when the pawn drives cellular material.

Walking uses gravity-relative up and slope handling. Swimming samples the
derived fluid field. Flying and noclip use their configured movement rules.
Pause normally stops world simulation; a pawn can opt into
`simulate_when_paused`.

## Contacts

The current public facade does not expose a general collision callback. Actor
contact events are the supported logical contact signal when available;
otherwise gameplay can compare fixed actor positions or query resident cells.
Use `Game::actor_contacts` for contact batches and keep game-specific overlap
state in the game layer.

See [Actors And Gameplay](actors.md), [World Cells And Edits](world.md), and
the internals [Rigid Cellular Bodies](
../internals/src/simulation/rigid-bodies.md
)
for ownership details.
