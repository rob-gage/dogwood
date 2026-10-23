+++
title = "Cellular kinematics"
description = "How the GPU moves dynamic cells through active tiles and exchanges momentum with actors and cellular rigid bodies."
weight = 30
[extra]
sources = ["engine_world/src/scenes/cellular_tiles", "engine_world/src/scenes/collisions/collision.rs", "engine_world/src/scenes/collisions/mod.rs"]
+++

This page is for engine contributors. It describes how `CellularTiles` advances dynamic cells in the active cellular tiles on the GPU, one tick of 1/60 second at a time, and how those cells interact with actors and cellular rigid bodies.

## Cell state

Each cell stores its material identifier, packed color, and packed kinematics state. The state holds velocity, progress toward neighboring cells, and two flags: resting and supported.

A two-bit material tag distinguishes dynamic cells. The remaining 30 bits select the material's mass, friction, and restitution record. Static identifiers and empty cells do not move.

## Movement and settling

The pass first collects tiles marked awake or disturbed, their neighboring tiles, tiles near selected actors, and tiles with nearby fluid/current. It then integrates every dynamic cell in those tiles.

### Integration

- An unsupported cell accelerates downward.
- A supported cell estimates its surface normal from nearby occupied cells. Gravity can accelerate it down a slope when there is space, while friction removes horizontal speed in proportion to gravity and the surface normal.
- A supported cell starts sliding when the slope exceeds what its friction can hold (approximately, when the slope tangent exceeds the friction coefficient) and there is room downhill.
- If both its horizontal and remaining vertical speed components fall below `RESTING_SPEED` (0.02 cells per tick), the cell stops and receives the resting and supported flags. This threshold also reduces rebound speed in cell-cell collisions. The cell then rests until a nearby change, actor, or fluid interaction makes its tile active again.
- The active-region edge supports cells and blocks movement beyond the region.

### Claiming and resolving moves

Velocity is clamped to eight cells per tick on each axis. The shader derives up to eight substep dispatches from cell progress, and the host limits submitted substeps using earlier control readbacks.

- For each enterable destination, the claimant moving fastest toward it wins.
- A hash of the destination cell, tick, substep, and candidate direction breaks ties.
- Resolve passes settle chains of cells that want to move together.

### Collisions

A later stage handles collisions between dynamic cells that remain in place.

- Effective restitution is the mean of both materials' restitution multiplied by a damping factor of `0.5`, so rebounds lose energy.
- Unbacked pairs exchange momentum using both cell masses. If either cell has backing behind it—the region edge, an actor, or matter not moving away this substep—the shader handles the collision as a bounce against a fixed cell.
- Against blockers, the moving cell's restitution controls rebound, and friction reduces sliding speed in proportion to impact-stopped speed.
- The moving cell's mass scales impulses passed to rigid-body walls.

## Actors and cellular rigid bodies

### Actor selection and upload

`CellularTiles` selects actors with position and collision components whose collision filter includes `COLLISION_GROUP_CELLULAR_TILE`, within four tiles of the active region. At most 32 actors are uploaded per tick.

- Fixed actors are assigned a very large simulation mass (`1.0e20`); kinematic and dynamic actors use their component mass.
- The host converts actor position changes since the previous call into cell-space per-tick velocities and clamps each axis to ±8 cells per tick before upload.
- The GPU tests overlap using actor shape and rotation, and uses the uploaded velocity in momentum exchange.

### Covered cells

When a dynamic cell is covered, it tries to move into a free neighboring cell. The preferred direction follows the actor surface's outward normal; for a rigid body, it follows the body's motion or points away from its center when the body is nearly still. If that cell is blocked, the shader tries the neighboring directions 45° to either side. A destination must be inside the active region and free of cells, rigid-body markers, and actors. If no destination is free, the cell stops; a rigid-body pusher also records a holding contact.

If the pusher would increase the cell's speed along the escape direction, the shader applies a mass-weighted velocity change and records the opposite impulse. Otherwise it makes no velocity exchange. The GPU also reports actor contact normals and penetration depths. The collision step consumes these contacts as described in [contact exchange](@/internals/simulation/contact-exchange.md).

### Rigid bodies

The pass receives rigid-body records and rasterized cells from the collision rigid-body pass. A body moving at or above `RIGID_BODY_RESTING_SPEED` (0.01 cells per tick) pushes covered cells according to its velocity; a slower body pushes them away from its center. Cells leaving a rigid-body cell position restore the body's stored material identifier and color there. See the [rigid-body lifecycle](@/internals/simulation/rigid-bodies.md) for ownership and rasterization.

The orchestration is in {{ <rustdoc path="engine/scenes/cellular_tiles/struct.CellularTiles.html" text="`CellularTiles`" /> }}. Entry-point order, packed layouts, and dispatch details belong to the [cellular tile shader reference](@/internals/shaders/cellular-tile-shader.md).

## Fluid interaction

Cellular integration reads the prior fluid raster, exchanges wet-face impulses, and adds buoyancy; separate kernels sample rigid-body fluid response even on inactive cellular tiles. [Fluid interactions](@/internals/simulation/fluid-coupling.md) owns those algorithms and their timing. Cellular rest flags and particle tile sleeping are independent.
