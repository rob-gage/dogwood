# Simulation Pipeline

`Scene::update` first services completed asynchronous work and queues streaming.
It then consumes accumulated elapsed time into fixed 60 Hz ticks. The following
is the current order inside a simulation tick when simulation is active:

1. Install the newest cellular collision snapshot and detach unanchored static
   components identified by the snapshot.
2. Synchronize pawn and physical-actor proxies into the CPU physics world.
3. Rebuild terrain colliders from the current cellular snapshot and activate
   rigid bodies whose staged collision origin is now valid.
4. Step the CPU physics world, then copy physical-actor transforms/velocities
   back into the actor registry.
5. Simulate actor pawns and collect deduplicated actor contact changes.
6. Rasterize actor and rigid-body proxies into the GPU cellular double.
7. Simulate fluids through prediction, neighborhood constraints, correction,
   contact, velocity reconstruction, and cellular rasterization.
8. Advect and force the gas velocity field before coupling.
9. Discover, arbitrate, and apply material reactions and resolve queued material
   mutations against the post-advection snapshot.
10. Solve cellular pressure, damage, transmission, and rigid granular contacts.
11. Resolve material mutations and consume fluid edits produced by prior stages.
12. Simulate dynamic/granular cellular movement.
13. Scatter mechanical fluid response and finish gas projection, concentration
    transport, and post-coupling state.
14. Gather thermal interaction, conduct heat, scatter temperatures across forms,
    evaluate phase transitions, and queue condensation/mutation requests.
15. Submit fluid sampling and rebuild the CPU-readable cellular collision view.

GPU command submissions are used to create ordering boundaries between groups.
The next update polls the device, applies mapped readbacks, applies rigid
mutations/detachments, and then starts the next fixed tick. This is why a result
such as a reaction or rigid phase transition is often observed one update after
its discovery rather than recursively inside the same pass.

When simulation is inactive, scene streaming and completion handling still run,
but the active simulation stages are skipped. Rendering can therefore show a
paused/editor scene without advancing its physical state.

### Relevant Implementation

- `engine_physics/src/scenes/scene_update.rs` — update prelude and tick order.
- `engine_physics/src/scenes/scene_cell_editing.rs` — completion application and
  representation changes.
- `engine_physics/src/simulation/mod.rs` — shared shader utilities and public
  simulation resource exports.
- `engine_physics/src/simulation/` — shared stage utilities and exports; the
  subsystem directories beside it own their individual dispatches.
