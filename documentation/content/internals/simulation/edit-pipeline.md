+++
title = "Edit pipeline"
description = "How cell edits are validated, coalesced, written to the GPU, and synchronized with collision and streaming."
weight = 90
[extra]
sources = ["engine_world/src/scenes/edits", "engine_world/src/scenes/scene.rs", "engine_world/src/scenes/collisions/collision.rs", "engine_world/src/scenes/collisions/collision_rigid_body_pass.rs", "engine_world/src/scenes/collisions/collision_rigid_body.wgsl"]
+++
This page explains how cell edits move from an `Edit` request through validation, coalescing, and the GPU edit pass, and how they interact with collision and streaming. It is for engine contributors; for the game-facing workflow see [editing cells](@/examples/editing-cells.md) and [collisions and edits](@/external-api/collisions-and-edits.md).

## Request and validation

An `Edit` groups a bit mask, material identifiers, and packed colors by cellular tile. `fill_cell` replaces an earlier fill at the same cell. `fill_segment` rasterizes cells whose centers lie within the requested radius (in tiles) of a segment.

| Limit or rule | Behaviour |
| --- | --- |
| `fill_segment` with a non-finite or negative radius | No-op; returns `Ok(())` |
| `fill_segment` candidate bounds span more than 4096 tiles | Returns `EditError::CapacityReached` |
| Queued tile entries exceed 4096 before the next stream | Whole request rejected |
| Tile outside the resident active region | Whole request rejected |
| Unregistered material | Whole request rejected |
| Fluid or gas material | Whole request rejected |

The first 4096 limit bounds one `fill_segment` call's candidates; the second bounds the queue between streams. Empty cells are allowed. `ScenePosition` uses integer coordinates and fixed-point offsets, so a segment's relative displacement is always finite.

### Color

Accepted non-empty colors pass through `MaterialTable::cellular_color`. Cells with available albedo use it tinted by the requested color; materials without albedo retain the requested color.

### Edit modes

- `EditMode::Insert` fills empty cells only. It skips cells overlapped by an actor's `ActorCollisionComponent` collider when its memberships include `COLLISION_GROUP_EDIT`; contact sensors do not mask edits.
- `EditMode::Replace` writes every requested cell, including empty material identifiers, and ignores colliders.

## Coalescing and GPU writes

At stream time, the edit state collects tiles touched by any Insert request and asks collision for per-tile masks of cells overlapped by those edit-group actor colliders. It processes queued edits in request order, merging per-cell state into one tile record.

- A Replace always takes effect and can clear an earlier Insert.
- An Insert writes only when its requested material is non-empty, it is outside the collider mask, no earlier Insert has claimed it, and no earlier Replace currently leaves a non-empty material there.

This preserves the sequential effect of the queued edits while needing only one merged record per affected tile.

The GPU edit pass uses one 64-invocation workgroup per tile. Replaced cells always write; inserted cells write only if the GPU cell is still empty. A write stores the material and color, clears the cell's kinematics, and marks its tile awake and disturbed. The edit state clears its queued tile count after processing; an empty merged result requires no GPU pass.

## Collision and persistence

When streaming produces a non-empty merged edit, an existing collision state requests a cell reread for its next simulation. If no collision state exists, the first collision simulation reads the active occupancy.

If rigid-body tiles are present, the next collision update waits for GPU readback before removing changed cells and splitting bodies; without them, it returns immediately. See [cellular rigid bodies](@/internals/simulation/rigid-bodies.md#edits-and-splitting) for the detachment conditions and split behavior.

Scene streaming writes queued edits before it moves the tile ring, so validated target tiles remain resident for the GPU write. Before a requested region change moves tiles, rigid bodies that leave that region are rasterized back into active cells. On scene drop, pending edits are streamed first, then active tiles are returned to chunk storage for saving; see [tile streaming](@/internals/world-streaming/tile-streaming.md).
