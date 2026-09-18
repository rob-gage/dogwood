# Recipes

## Create a game

Implement `Game`, store `Option<Scene>` and `UserInterfaceContext`, then call
`MyGame::launch(Arc::new(Accelerator::new()?))`. See
`template_project/src/project.rs`.

## Register materials

Use `MaterialRegistryBuilder::new`, `register`, `set_thermal`, optional
`tag`/`register_reaction`, then `compile`. Pass the registry to `Scene::new`.

## Spawn and update an actor

Call `spawn` or `spawn_pawn`, retain the returned `Actor`, and read/update it
through `ActorRegistry`. Use `set_position`, `set_velocity`, and
`set_control_state`; call `despawn` when it leaves the game.

## React to player contact

Get `scene.possessed_actor()`, read both positions, compare distance against the
pickup radius, then `despawn` the pickup and increment a counter. The current
facade has no collision-event callback.

## Query nearby cells

Convert a tile-space point with `CellCoordinates::from_world_position`, then
inspect `scene.tile_at(cell.tile_coordinates())` if that tile is resident.
Download a `TileArea` when you need full CPU cell state.

## Remove or modify a region

Build a `Vec<CellCoordinates>`, call `SceneEditBatch::erase`, `destroy_cells`,
or `thermal`, and pass the batch to `scene.queue_edits`.

## Make a collectible

Spawn a plain actor at its position, keep its `Actor` in game state, check
player distance each update, then despawn it on pickup. This uses only public
APIs and avoids a speculative item framework.

## Add a counter

Keep `score: u32` on the game, and render it with `UserInterfaceContext::run`
plus `ui.egui().label(format!("Score: {}", self.score))`.

## Save/load persistent state

Use `SceneData::load` and `Scene::load` for world state. Persist game-specific
counters in your game’s own save file; Dogwood’s scene data is not a
general-purpose player-profile store.
