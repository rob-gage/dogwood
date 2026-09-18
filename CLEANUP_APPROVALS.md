# Cleanup approval ledger

This file records cleanup proposals that are intentionally not applied because
they could change the public API or remove code that may be needed by a future
Dogwood system. Approval is required before implementing any item here.

## Public API proposals

| Symbol or area | Current state | Proposed change | Reason to defer |
| --- | --- | --- | --- |
| `ChunkFluidParticle::GPU_SIZE` | Public compatibility constant in `engine_physics/src/chunks/chunk_fluid_particle.rs` | Rename to Accelerator terminology | Public name change |
| `Material::Fluid::xsph_smoothing` | Public serialized material field | Rename to neighbor velocity smoothing | Public field and serialized-format change |
| Public material `graphics` fields | Used by demo and material authors | Consider `appearance` terminology | Public field change |
| Public simulation exports in `engine_physics/src/simulation/mod.rs` | Includes implementation systems such as `Fluids`, `Gases`, `CellularPressure`, `CellularCollision`, and `MaterialMutations` | Internalize implementation types where possible | External imports must be audited first |
| Compiled reaction exports | Compiled reaction types and helpers are publicly re-exported from `materials` | Internalize authoring-implementation details | Public import compatibility |
| Scene transfer and streaming types | Upload/download and chunk-streaming implementation types are exposed through engine modules | Internalize implementation resources | Public module-path compatibility |
| `SceneSimulation` | Public trait in `scene_simulation` with `Scene` as its only implementation | Replace with private, named simulation components | Public trait and module-path change |
| `MaterialMutations::new` | Public constructor retains an unused `gas_velocity` parameter | Remove the stale parameter | Public signature change |
| `Accelerator` compatibility names | `wgpu_*` accessors are public and `ChunkFluidParticle::GPU_SIZE` remains public | Consider broader Accelerator terminology | Existing callers and public API |

No proposal in this section has been implemented by the cleanup sprint.

## Dead-code review

These are not deletion approvals. They are current candidates requiring a
future-system review.

| Symbol or file | Current references | Why it appears unused | Recommendation |
| --- | --- | --- | --- |
| `SceneData::is_temporary` in `scene_data_store.rs` | Set by `load` and `new_temporary`; no read reference found | Temporary-directory ownership state is stored but not consumed | Retain until temporary-scene cleanup semantics are explicitly decided |
| `Scene::rigid_cell_temperatures_buffer` in `scenes/scene.rs` | Definition only | No current caller found | Review with future rigid thermal readback work before removal |
| `CellParticle::material_identifier` in `simulation_cellulars/cell_particle.rs` | Type is re-exported; field has no current read | Public cellular particle contract may be future-facing | Retain pending API review |
| `MaterialReactions` fields and `reaction_energy_buffer` | Owned by `Scene`; resource fields are consumed by GPU dispatch/readback paths | Several fields are not read directly by Rust | Do not delete; verify shader/binding ownership before any change |
| `RigidGranularReactionBatch` fields | Constructed and consumed through pressure readback | Some decoded fields are not read by current Rust callers | Retain for reaction/readback compatibility |
| `ScenePhysicsWorld::apply_rigid_support` and `apply_rigid_recovery` | Definition only | Current pressure path uses other reaction application paths | Review against planned rigid support/recovery integration |
| Thermal helper methods in `simulation_thermal` | Definitions only for `conduct`, `gather`, `evaluate`, and `scatter` | Current scene tick uses the newer dispatch path | Retain until the thermal API is finalized |
| `TileData` cell accessors | Definitions only in current production callers | Useful narrow test/future interface | Retain until tile access ownership is reviewed |

The current workspace check and test suite pass with all candidates retained.
