+++
title = "Error handling"
description = "How Dogwood represents failures, propagates them, and turns some into panics."
weight = 60
[extra]
sources = ["engine/src/runtime/engine_error.rs", "engine_world/src/scenes/scene_error.rs", "engine_world/src/actors/actor_error.rs", "engine_world/src/materials/material_error.rs", "engine_world/src/scenes/chunks/chunk_error.rs", "engine_world/src/scenes/edits/edit_error.rs", "engine_graphics/src/sprites/sprite_sheet_error.rs", "engine_host/src/acceleration/accelerator_error.rs", "engine_host/src/acceleration/accelerator.rs", "engine_host/src/processors/processor_error.rs", "engine_host/src/processors/processor.rs", "engine_host/src/windows/window_error.rs", "engine/src/runtime/engine.rs", "engine/src/games/game_instance.rs", "engine_world/src/scenes/scene.rs", "engine_world/src/scenes/chunks/chunk_streaming_state.rs", "engine_world/src/scenes/edits/edit_streaming_state.rs"]
+++

This page covers the error types, how they convert and propagate, and where the runtime turns them into panics. It is for engine contributors.

## Error layers

Errors are returned at the API that can report them. {{ <rustdoc path="engine/runtime/enum.EngineError.html" text="EngineError" /> }} is the launch-level wrapper, with variants for accelerator, processor, window, sprite-sheet, material, and actor failures. Its `From` implementations convert exactly those six error types. `Engine::launch` returns `Result<(), EngineError>`.

Scene operations use {{ <rustdoc path="engine/scenes/enum.SceneError.html" text="SceneError" /> }} for chunk streaming and storage, camera actor lookup, tile readback and poll, collision/kinematics, actor despawn, and collider validation failures.

`SceneError` has no general `From` conversions: call sites wrap lower-level errors explicitly, such as `CameraActor { error }` and `ActorDespawn { error }`. See the [engine and game API](@/external-api/engine-and-games.md), [scene API](@/external-api/scenes.md), and [editing guide](@/examples/editing-cells.md).

```text
 Source errors                             Wrapper
+------------------+   From              +-------------+
| AcceleratorError |-------------------->|             |
| ProcessorError   |                     | EngineError |-- Engine::launch
| WindowError      |                     |             |
| SpriteSheetError |                     +-------------+
| MaterialError    |
| ActorError       |
+------------------+

ChunkError + coordinates -------------> SceneError
ActorError + scene context -----------> SceneError
GPU/collision/kinematics errors ------> SceneError
Scene::edit ──returns──> EditError
```

Arrows point from source errors to wrappers, except `Scene::edit ──returns──> EditError`, which shows its direct return value, not a conversion.

`Engine::launch` itself only produces `EngineError::Window`. The other variants exist for callers who construct the engine's resources themselves.

## Error categories

The enums identify failures; they do not decide whether a caller can recover.

| Error type | Failures covered |
| --- | --- |
| `ActorError` | Exhausted identifiers, unloaded or despawned actors, and missing components. |
| `MaterialError` | Capacity, empty names, identifier range, and compilation. |
| `SpriteSheetError` | Image decoding and invalid dimensions. |
| `AcceleratorError` / `ProcessorError` | Graphics initialization and pool configuration or build failures. |
| `WindowError` | Event-loop, OS, surface creation, unsupported surface, and surface validation failures. |
| `ChunkError` | I/O and serialized chunk validation. |
| `SceneError` | Streaming, storage, tile readback, collision/kinematics, despawn, and collider failures; chunk load, save, and generation variants carry coordinates and a lower-level error. |
| `EditError` | Invalid or unsupported edits, or edit request capacity reached. |
| `EngineError` | Launch wrapper; see above. |

### Recovery and retry

- Failed dirty saves keep their chunk for retry. `retry_chunk_streaming_request` retries dirty saves with the retained chunk, and retries failed loads or generations only while their coordinate is still in a requested region.
- `Scene::stream` wraps camera lookup failures in `SceneError::CameraActor`; `Scene::graphics` returns `ActorError` directly.
- `Scene::edit` returns `EditError` for inactive tiles, unregistered or unsupported materials, or request capacity.

These remain ordinary results until a caller propagates, handles, or panics on them.

## Panics at runtime boundaries

Several runtime paths choose to turn errors into panics:

- `Engine::default` constructs the default accelerator and processor; their `Default` implementations panic if initialization fails. `Engine::new` accepts already-created resources.
- `GameInstance::update` panics if pawn possession insertion, scene streaming, or scene simulation fails. Its `stream` method panics on streaming failure. Rendering panics if scene graphics lookup fails.
- `Scene::drop` panics if applying queued cellular edits, returning GPU-resident tiles, or returning fluid imports/live particles fails.
- `ChunkStreamingState::drop` waits for outstanding saves, retries each owned dirty chunk up to three attempts, and panics if saving still fails.

An error may therefore be recoverable at a direct API boundary while the engine runtime chooses a fatal policy when it drives that API. The drop sequence is described in [saving and drop behavior](@/architecture/resources-and-lifetimes.md#saving-and-drop-behavior).

## Fluid failures

`SceneError` includes invalid or fixed fluid capacity, particle mapping/poll failures, invalid export sizes or records, inactive particle destination chunks, closed snapshot channels, and missing tile streaming request identifiers. Chunk particle validation reports capacity, material, position, and velocity errors. See [chunk format](@/internals/world-streaming/chunk-format.md). Scene drop panics if fluid particles cannot be returned to chunk storage.
