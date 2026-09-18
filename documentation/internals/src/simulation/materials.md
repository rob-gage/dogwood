# Materials And Reactions

Materials are the semantic contract shared by simulation, persistence, and
rendering. The CPU registry owns authored definitions and identifiers. The
compiled GPU tables make those definitions available to cellular, fluid, gas,
thermal, pressure, and reaction passes without each pass reinterpreting Rust
configuration.

## Material Forms

`Material` has four forms: static cellular, dynamic cellular, fluid, and gas.
The form is a major simulation distinction, not merely a rendering label. It
selects the authoritative representation and the solver that can move or
mutate the material. Rigid cellular cells use cellular material definitions but
retain an additional body-local owner.

## Material Identification

`MaterialIdentifier` contains a form-local index. The registry keeps separate
form collections and provides a dense lookup used when packing metadata for the
GPU. A dense index is not a world coordinate and must not be persisted as a
substitute for the identifier; serialization preserves the registry and its
stable form/index meaning.

### Relevant Implementation

- `engine_physics/src/materials/material.rs` — authored form-specific material
  definitions.
- `engine_physics/src/materials/material_form.rs` — material-form enum.
- `engine_physics/src/materials/material_identifier.rs` — packed identifier
  encoding and form/index access.
- `engine_physics/src/materials/material_registry.rs` — CPU registry, lookup,
  dense mapping, and public inspection.

## Material Registry

`MaterialRegistry` is the CPU authority for the lifetime of material metadata.
It owns form collections, thermal properties, tags, reaction records, and the
mapping from identifiers to dense records. A scene builds material-derived
GPU resources from this registry during construction. Simulation stages borrow
those compiled resources; they do not independently register materials.

## Material Registry Builder

`MaterialRegistryBuilder` is the authoring boundary. Game or template code
registers materials first, then assigns thermal properties and tags, then
registers reactions and calls `compile`. Compilation validates references,
form compatibility, thresholds, quantities, and selectors before producing a
registry. This is intentionally front-loaded: GPU stages can use packed data
without repeating authoring validation every tick.

### Relevant Implementation

- `engine_physics/src/materials/material_registry_builder.rs` — authoring API
  and compilation entry point.
- `engine_physics/src/materials/material_registry_validation.rs` — validation
  of materials, thermal metadata, tags, and reactions.
- `template_materials/src/materials.rs` — complete canonical material set.
- `template_materials/src/reactions.rs` — canonical reaction declarations.

## Compiled Material Metadata

Compilation turns form-local definitions into dense material records and packed
GPU buffers. Records contain the values needed by simulation, including mass,
pressure behavior, fluid/gas transport properties, thermal properties, and
appearance-facing values. The GPU sees a stable table indexed by compiled
dense identity; the scene uploads the table once when constructing its
materials and refreshes it only when the scene's registry changes.

### Relevant Implementation

- `engine_physics/src/materials/material_table.rs` — packed simulation metadata
  and selector-member buffers.
- `engine_physics/src/materials/material_registry_graphics.rs` — conversion to
  render-facing material graphics.
- `engine_graphics/src/material_graphics.rs` — packed appearance/graphics data.

## Tags

Tags name groups of material identifiers. A reaction selector can use a tag to
target a family without declaring every pair separately. The registry expands
tag membership during compilation, so GPU candidate discovery can use compact
selector records rather than traversing dynamic Rust collections.

## Reaction Definition

A reaction declares reactants and products, quantities, environmental gates,
priority, extent/rate, thermal energy, and pressure output. Reactants and
products identify material forms and quantities; gates can constrain
temperature, pressure, air, or other compiled environment values. Exact
selectors name one material, while tag selectors name a compiled group.

## Reaction Discovery

During simulation, the reaction stage looks at the current material authority
and gathers candidate rules whose selectors and environmental gates match.
Candidates may originate in cellular cells, fluid particles, gas species, or
rigid-cellular proxies, but the candidate records retain the authority needed
to apply the result to the correct form.

### Relevant Implementation

- `engine_physics/src/materials/material_reaction.rs` — authored reaction
  model.
- `engine_physics/src/materials/material_selector.rs` — exact and tag selector
  model.
- `engine_physics/src/materials/compiled_material_reaction.rs` — dense GPU
  reaction representation.
- `engine_physics/src/simulation_materials/material_reactions_dispatch.rs` —
  candidate dispatch and application ordering.

## Reservation And Arbitration

Several reactions can observe the same reactant during one pass. Applying all
of them would create material from matter that was consumed twice. Candidate
reservation therefore arbitrates competition: candidates claim the required
source amount in priority/order, and later candidates see the reservation
remaining rather than the original amount. The GPU reservation passes keep
this decision parallel while making consumption bounded and deterministic
enough for the compiled ordering.

This is separate from reaction discovery. Discovery answers “could this rule
apply?”; reservation answers “which candidates are allowed to consume this
amount?”

### Relevant Implementation

- `engine_physics/src/simulation_materials/material_reactions.rs` — reaction
  resources and high-level lifecycle.
- `engine_physics/src/simulation_materials/material_reactions_construction.rs` —
  candidate and reservation buffer construction.
- `engine_physics/src/simulation_materials/
  material_reactions_shader_candidate_reservation.wgsl` —
  candidate arbitration.
- `engine_physics/src/simulation_materials/
  material_reactions_shader_reservation.wgsl` —
  source-amount reservation.

## Reaction Application

Application consumes the reserved reactant amount, generates products in their
target forms, and emits pressure and thermal energy. A product can therefore
be a cellular material, a fluid particle, or a gas species. The mutation and
readback paths preserve the source authority and validate asynchronous rigid
results before changing body-local cells.

## Material Mutations

`MaterialMutations` is shared routing infrastructure for phase changes,
reactions, edits, and rigid-cell outcomes. A request identifies its source and
target form, amount, temperature/energy effects, and location or owner. The
resolver dispatches to the cellular field, fluid allocator, gas species field,
or rigid body-local state. This prevents a convenient cell-space proxy from
being mistaken for the authority of another form.

### Relevant Implementation

- `engine_physics/src/simulation_materials/material_mutations.rs` — mutation
  queue and cross-form resolver.
- `engine_physics/src/simulation_materials/material_mutations.wgsl` — GPU-side
  form-specific mutation application.
- `engine_physics/src/simulation_materials/material_reactions_readback.rs` —
  bounded readback of reaction effects.
- `engine_physics/src/scenes/scene_rigid_cell_mutation.rs` — applies validated
  rigid-cell mutation results.

## Exact Materials Versus Tags

Exact selectors are precise and cheap when a rule has one intended source.
Tags reduce authoring duplication and let a rule apply to a family, at the
cost of a larger compiled membership set and more candidate matches. Both are
resolved during registry compilation; runtime code receives dense selector
records rather than walking tag strings.

## How Material Forms Work Together

The forms share identifiers and semantics but do not share one authority:

| Form | Authoritative resident representation |
|---|---|
| Static cellular | Resident cellular fields, downloaded to `TileData` |
| Dynamic cellular | Resident moving-cell fields, downloaded with tile state |
| Fluid | GPU particle records |
| Gas | GPU Eulerian velocity/concentration/temperature fields |
| Rigid cellular | CPU body-local cells and transform |

The common cell domain is an interaction surface. Static and dynamic cells live
there directly; rigid bodies and actors are rasterized there; fluid particles
produce a coverage/velocity/temperature raster; gas uses its own grid and
mutation interfaces. Render views, collision snapshots, rasterizations, and
readbacks are all derived representations. They are useful because neighboring
systems can exchange contact, heat, pressure, and material identity without
sharing ownership.

Thermal coupling gathers these views into a cell-space solve and scatters the
result back to each authority. Mechanical coupling uses static occupancy,
cellular pressure, body proxies, fluid contacts, and CPU Rapier state. Reactions
can consume one form and create another because compiled products carry a form
and mutation routing carries an authority locator.

Conceptually, ice to water changes a cellular authority into fluid particles;
water to vapor changes particles into gas concentration; vapor cooling to water
does the reverse; stone to lava can replace static cells with a fluid product;
and a rigid stone cell melting into fluid removes a body-local cell only after
validated readback. None of these transitions is implemented by changing a
proxy alone.

Ordering is therefore essential. The scene applies edits and completed
readbacks, updates body/actor proxies, runs form-specific simulation, gathers
thermal and reaction effects, scatters thermal results, evaluates phase
changes, and applies cross-form mutations before the next representation is
uploaded. The exact dispatch grouping is in [Simulation
Pipeline](pipeline.md), while ownership details are in [Ownership,
Coordinates, And CPU/GPU Split](../architecture/ownership.md).
