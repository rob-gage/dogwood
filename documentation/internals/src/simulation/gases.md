# Gases

Gases use an Eulerian field over resident cells. One shared velocity field and
temperature field are paired with species-major concentration fields. Each gas
material is a species, so multiple gases occupy the same flow grid while
retaining separate concentrations. Gas concentration is authoritative while
resident; chunks store sparse dormant records for streaming.

## Gas Representation

The velocity field describes the flow shared by all species. A concentration
field describes how much of one registered gas species occupies each cell.
Material metadata supplies density, diffusivity, optical extinction,
dissipation, and compressibility. Temperature is shared by the gas grid and is
also visible to thermal and reaction stages.

This representation differs from fluids: gas does not allocate a particle per
parcel. It advects fields through fixed cell locations, which makes the
representation suitable for smoke-like volumes and shared flow.

### Relevant Implementation

- `engine_physics/src/simulation_gases/gases.rs` — velocity, concentration, and
  temperature buffers.
- `engine_physics/src/simulation_gases/gases_construction.rs` — gas field
  allocation and material-count sizing.
- `engine_physics/src/materials/material.rs` — gas material metadata and form.

## Velocity Advection

The previous velocity field is transported through the resident grid to create
the next flow field. Advection carries motion through space rather than
teleporting each species independently. The solver then applies forces and
coupling before pressure projection, so the velocity used to move species is
the current shared flow rather than an independent per-species velocity.

## Vorticity And Buoyancy

Curl measures local rotation in the velocity field. Vorticity confinement
reintroduces small-scale swirl lost to grid dissipation without changing the
authority model. Gravity and buoyancy then respond to gas density and thermal
state: density differences and hot/cold gas influence vertical acceleration
where material metadata enables it. These are qualitative flow forces; the
material registry remains the source of species properties.

### Relevant Implementation

- `engine_physics/src/simulation_gases/gases_operations.rs` — pre/post-coupling
  ordering and field dispatch.
- `engine_physics/src/simulation_gases/gases_shader_solver.wgsl` — advection,
  curl, force, pressure, and concentration solver stages.

## Divergence And Pressure Projection

After velocity forces, the solver measures divergence: how much flow locally
expands or contracts. It solves for a pressure correction through repeated
Jacobi iterations, then subtracts the pressure gradient from velocity. This
produces approximately incompressible shared motion while leaving
material-specific concentration compressibility to species transport.

The iterations are GPU passes over scratch fields. They are not a CPU physics
world and are not persisted. Rendering and reactions consume resident fields
only after the relevant dispatch ordering has completed.

## Species Transport

Each species concentration is advected by shared velocity, diffused according
to its diffusivity, and reduced by dissipation. Compressibility controls how
concentration responds to local flow and material rules. Species can therefore
share one velocity field while spreading and disappearing at different rates.

## Interaction With Thermal And Material Systems

Reactions can create or consume gas species. Phase transitions can turn fluid or
cellular material into gas, or request condensation into another form. The
mutation system carries an authority locator so the change targets the gas
concentration field rather than an unrelated cellular cell. Gas temperature
participates in thermal interaction and is scattered back to the gas field
after cross-form conduction.

### Relevant Implementation

- `engine_physics/src/simulation_materials/material_mutations.rs` — routes
  cross-form material changes.
- `engine_physics/src/simulation_materials/material_reactions.rs` — discovers
  and applies reaction effects.
- `engine_physics/src/simulation_thermal/thermal_scatter.rs` — propagates
  resolved temperature to gas cells.

## Gas Rendering

The renderer samples resident gas concentrations and material appearance data.
Concentration controls presence/opacity-like contribution while the material
graphics table supplies color and other appearance properties. Rendering is a
derived read; it never edits gas concentration or reconstructs dormant gas.

## Streaming

Outgoing gas cells are compacted into sparse `ChunkGasCell` records with their
species and thermal state. Incoming records restore concentrations into the
resident field after the ring origin and material registry are ready. A stale
download cannot write to a reused physical region because scene transfer jobs
validate their area and generation before applying results.

### Relevant Implementation

- `engine_physics/src/simulation_gases/gases_streaming.rs` — export/import
  packing.
- `engine_physics/src/chunks/chunk_gas_cell.rs` — dormant gas record format.
- `engine_physics/src/scenes/scene_gas_streaming.rs` — scene transfer boundary.
- `engine/src/renders/scene_renderer.rs` — render binding for gas fields.
