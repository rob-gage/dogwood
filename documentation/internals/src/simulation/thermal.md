# Thermal simulation

Temperature is stored per cellular cell, fluid particle, rigid cell, and gas
physical cell. Material thermal metadata supplies conductivity, specific heat,
default temperature, and optional cold/hot transitions. Empty space has scene
configuration for ambient temperature, conductivity, and heat capacity.

Thermal interaction first gathers cross-form contact/energy contributions,
including one-tick reaction energy. Conduction solves neighboring-cell heat
transfer. Thermal scatter then propagates resolved temperature between the
cellular, fluid, rigid, and gas representations; rigid temperature uses a
bounded accumulation/apply path because body-local cells overlap the rasterized
cell domain.

Phase evaluation runs after scatter. Cell, fluid-particle, gas, and rigid
candidate passes compare temperature to material transition thresholds. A
transition carries target material, yield rate, and latent energy. Candidates
become `MaterialMutations`; rigid candidates require an asynchronous bounded
readback before body-local membership and debris are changed.

Thermal edits are sparse externally-authored deltas routed to the authoritative
representation. The scene applies them through the same GPU resources rather
than modifying a CPU copy that could be overwritten by the next upload.

The ordering matters: conduction observes current material/temperature state;
scatter makes the result visible across forms; phase transitions request
material changes; mutation resolution applies those requests and can generate
fluid/gas condensation. Latent energy and reaction energy are inputs to this
cycle, not immediate recursive phase transitions.

### Relevant implementation

- `engine_physics/src/simulation_thermal/thermal_interaction.rs` — gathers
  cross-form thermal interaction and reaction energy.
- `engine_physics/src/simulation_thermal/thermal_conduction.rs` — conduction
  solve and resolved thermal buffer.
- `engine_physics/src/simulation_thermal/thermal_scatter.rs` — cross-form
  temperature synchronization.
- `engine_physics/src/simulation_thermal/thermal_phase_transitions.rs` —
  threshold evaluation and rigid candidate readback.
- `engine_physics/src/simulation_thermal/thermal_edits.rs` — external thermal
  edit application.
- `engine_physics/src/materials/material_thermal_properties.rs` — authored
  thermal metadata model.
