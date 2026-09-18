# Thermal Simulation

Thermal simulation couples the forms without merging their authorities.
Cellular temperature lives in cell fields, fluid temperature lives in particle
records, gas temperature lives in the Eulerian gas field, and rigid-cell
temperature belongs to body-local cells. A resolved cell-space thermal field
is the temporary meeting point for conduction and cross-form interaction.

## Temperature Authorities

Active static and dynamic cells carry temperature beside material, amount, and
integrity. Fluid particles carry their own temperature because particles can
move between cells. Gases share one temperature field for the gas grid rather
than storing a temperature per species. Rigid bodies keep temperature with
body-local cells; their rasterized cellular temperature is only a derived
double.

This distinction prevents a later upload from overwriting an authoritative
change with an old proxy. Every scatter or mutation path identifies its target
form before writing.

### Relevant Implementation

- `engine_physics/src/simulation_thermal/thermal_interaction.rs` — gathers
  cross-form thermal inputs and reaction energy.
- `engine_physics/src/simulation_thermal/thermal_conduction.rs` — resolves the
  cell-space heat field.
- `engine_physics/src/simulation_rigid_bodies/rigid_cellular_body_cell.rs` —
  body-local cell temperature ownership.
- `engine_physics/src/simulation_fluids/fluids.rs` and
  `engine_physics/src/simulation_gases/gases.rs` — fluid and gas authorities.

## Thermal Interaction Gathering

The interaction stage gathers temperatures and heat contributions from nearby
forms. Material reactions add or remove thermal energy as part of the same
input. Rigid-cell contributions are accumulated because several body-local
cells can overlap the same raster cell, while fluid and gas fields contribute
through their derived cell-space views.

## Conduction

Conduction transfers heat between neighboring occupied cells according to
material conductivity and specific heat capacity. Empty space has configured
thermal behavior as well, so a cell can exchange heat with its surroundings
without inventing a new material. The resolved result is kept in a dedicated
thermal buffer until the cross-form scatter stage.

Conduction happens before phase evaluation. This lets a hot neighbor bring a
material across a transition threshold during the same thermal cycle instead
of evaluating a stale, pre-conduction temperature.

### Relevant Implementation

- `engine_physics/src/simulation_thermal/thermal_conduction.rs` — conduction
  resource and dispatch.
- `engine_physics/src/simulation_thermal/thermal_conduction.wgsl` — neighbor
  heat transfer.
- `engine_physics/src/materials/material_thermal_properties.rs` — authored
  conductivity, heat capacity, and default temperature.

## Thermal Scatter

Scatter propagates the resolved cell-space temperature back to each
authoritative representation. Cellular fields receive direct updates. Fluid
particles receive temperature sampled from their cells. Gas receives the
resolved gas-grid temperature. Rigid bodies use an accumulation and apply path
that maps raster results back to body-local cells while respecting body
identity and residency.

### Relevant Implementation

- `engine_physics/src/simulation_thermal/thermal_scatter.rs` — cross-form
  scatter ordering and buffers.
- `engine_physics/src/simulation_thermal/thermal_scatter.wgsl` — GPU scatter
  operations.
- `engine_physics/src/simulation_rigid_bodies/rigid_cell_state_gather.rs` —
  rigid-cell result transfer support.

## Phase Transition Evaluation

Material thermal metadata can define cold and hot transitions. After scatter,
the phase stage compares authoritative temperature with those thresholds and
creates candidates carrying target material, yield, and latent energy. Yield
limits how much material changes during the step; latent energy adjusts the
thermal budget rather than recursively forcing unlimited transitions.

Candidates do not mutate material immediately. They enter the shared mutation
path, which resolves the target representation. Rigid candidates use bounded
readback before body-local topology is changed.

## Cross-Form Phase Changes

A transition can stay within cellular matter or move material between static
cellular, dynamic cellular, fluid, gas, and rigid-cellular state. The phase
stage identifies the source authority, then mutation application consumes the
source and creates the target form. A rigid proxy never becomes the source of
truth for that decision; the body-local cell is read or changed through its
validated owner.

## Thermal Edits

External thermal edits are sparse requests against cell coordinates. The scene
queues them with other edits and applies them through thermal GPU resources,
then scatters the result to the relevant authority. This avoids changing a
CPU tile copy that a later resident upload could overwrite.

## Ordering

The thermal sequence is: gather temperatures and reaction energy; resolve
neighbor conduction; scatter the resolved result to cellular, fluid, gas, and
rigid authorities; evaluate cold/hot candidates; and apply material mutations.
The scene then carries the resulting material and latent-energy changes into
the next coupled stages. This order is why reactions and phase changes can
share energy without creating an immediate recursive loop.

### Relevant Implementation

- `engine_physics/src/simulation_thermal/thermal_edits.rs` — external thermal
  edit application.
- `engine_physics/src/simulation_thermal/thermal_phase_transitions.rs` —
  candidate generation and rigid readback coordination.
- `engine_physics/src/materials/material_thermal_transition.rs` — transition
  target, threshold, yield, and latent-energy metadata.
