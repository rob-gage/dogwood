# Materials and reactions

## Material authoring and compilation

`Material` is the CPU authoring enum with four forms: static cellular,
dynamic cellular, fluid, and gas. `MaterialIdentifier` encodes form and a
form-local index. `MaterialRegistry` stores separate form vectors, thermal
metadata, tags, and compiled reaction records. Its dense form-major index is
the stable lookup used by packed GPU tables.

`MaterialRegistryBuilder` accepts materials first, then thermal metadata, tags,
and declarative reactions. `compile` validates references and numeric gates,
resolves exact selectors and tags, and produces fixed-stride compiled records.
The registry remains the CPU source of truth; `MaterialTable` and
`MaterialGraphics` pack immutable derived metadata into Accelerator buffers for
simulation and rendering.

## Reactions

A reaction has up to two reactants and products. A reactant selects an exact
identifier or a tag-expanded set and consumes an amount. Environmental gates
can constrain temperature, retained pressure, and air. Priority orders
competing candidates; maximum extent bounds work per cell per tick. Products,
thermal energy, and pressure output are the observable results.

GPU reaction discovery scans the current post-advection material snapshot,
reserves candidate capacity, compacts and sorts candidates, then performs an
ordered arbitration pass. The serialized reservation order is a correctness
rule: two reactions cannot consume the same material. Application writes
cross-form `MaterialMutations` and reaction energy, while rigid removals are
read back through bounded staging storage.

`MaterialMutations` is the common form-agnostic replacement queue. It resolves
cell replacements without allocation when possible and has separate handling
for gas/fluid condensation. Its request format carries authority information so
the result is applied to the owning form rather than only to the visual cell
double.

### Relevant implementation

- `engine_physics/src/materials/material.rs` — authored form-specific material
  properties.
- `engine_physics/src/materials/material_registry.rs` — registry, dense index,
  validation, tags, and reaction compilation.
- `engine_physics/src/materials/material_registry_builder.rs` — declarative
  authoring builder.
- `engine_physics/src/materials/material_table.rs` — immutable GPU thermal and
  reaction metadata packing.
- `engine_physics/src/simulation_materials/material_reactions.rs` — reaction
  resources, buffers, and pipeline construction.
- `engine_physics/src/simulation_materials/material_reactions_dispatch.rs` —
  discovery, arbitration, and application dispatch order.
- `engine_physics/src/simulation_materials/material_mutations.rs` — replacement
  and condensation request resolution.
