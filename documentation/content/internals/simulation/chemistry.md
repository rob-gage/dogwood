+++
title = "Chemistry simulation"
description = "Reaction units, block ownership, rates, mutation, and limits."
weight = 45
[extra]
sources = ["engine_world/src/scenes/chemistry", "engine_world/src/scenes/scene.rs", "engine_world/src/materials", "engine_world/src/scenes/thermal", "engine_world/src/scenes/fluids", "engine_world/src/scenes/pressure"]
+++

Chemistry applies registered reactions to cellular matter, gas, and fluid particles in cells selected by the thermal simulation. It records its compute work inside thermal's compute pass after thermal prepares and before thermal steps, so reaction products and heat are handled during that tick's thermal step. The [chemistry shader reference](@/internals/shaders/chemistry-shader.md) describes the GPU stages and bindings.

## Reactant units and amounts

A cell exposes seven reaction units: its cellular matter, four gas slots, and two fluid species slots. Cellular matter and fluid particles use 65,536 amount units per full cell or particle; gas uses 4,096 units per atmosphere. Material names and tags select units. A reactant amount of zero marks a catalyst; catalysts are required but not consumed. Consumed reactant amounts are at least 1/4096, so no event rounds its consumption to nothing.

One reaction has one or two reactants and zero to three products. Event extent is `ε = min(1, available / required)` across consumed reactants. A shortage therefore applies a partial event: consumed amounts, cellular or gas product amounts, heat, and added pressure scale by `ε`, with amounts updated using integer accounting. A reaction that produces fluid requires a whole event; a partial extent does not run. The minimum applied extent is `1.0e-5`. Products inherit transformed color and radiance from the selected consumed reactants. A product amount is in cellular/fluid units or atmospheres for gas.

Temperature and pressure requirements use the means across the reactant cells; pressure includes phantom air. The inclusive windows default to 0 K and unbounded pressure/temperature maxima. A reaction can add heat per event, including negative heat, and add a nonnegative pressure amount in atmospheres. Gas products go into the cell of the first consumed gas reactant. Without one, they go into a cell free of cellular matter after the event: a reactant's cell, then a block cell sharing a face with the first consumed reactant. Matter therefore burns at its surfaces, and a reaction making gas or pressure with no such cell waits. Pressure output is added to the gas cell as well. Heat is shared between the consumed reactants' cells and the gas cell by the heat capacity left in them, and warms each cell's gas and cellular matter directly, like a flame above a cooler liquid; a cell holding only fluid passes its share to thermal, which warms its particles. A blocked full gas slot blocks the event. Cellular products need an empty cellular-matter slot; fluid occupying that cell is pushed out by the solver. Fluid products enter the bounded spawn list, whose records an event reserves before it changes anything, and are consumed by the fluid pass on the next tick. An event uses up at most one rigid body cell, which leaves its body through the collision-owned change list.

## Block ownership and rates

The active cell grid is partitioned into 2×2 blocks. The block offset alternates between `(0, 0)` and `(1, 1)` each tick. A block is owned by the tile containing its lower-left cell; one invocation processes its four cells in sequence, so later contacts see amounts left by earlier reactions. Unary and same-cell reactions are checked within the block. Two-reactant contacts across its internal faces run every other tick, so their event probability uses twice the registered rate times the tick duration. Catalytic contacts also inspect the eight surrounding cells read-only, and are checked across every face each tick.

For a checked contact, the event probability is `1 - exp(-rate * dt)` (with the two-sided scaling above). A deterministic hash of cell, tick, and reaction seeds the roll. When several reactions match one contact, registration order defines priority.

A block cell in a tile thermal did not list this tick counts as empty, so every change lands where that tick's thermal apply pass takes it in. `select_tiles` marks active thermal-listed tiles that can react, reading what each cell holds and conservative partner-class masks; `prepare_reactions` builds an indirect dispatch; `react_tiles` processes only selected tiles and blocks. A possible reaction keeps the tile thermally listed even when its random roll fails. This reuses thermal's awake/listed path and lets the next thermal step conduct reaction heat and process transitions. During reaction traversal, unary flags can skip a first-unit row with no possible unary reaction, and conservative class masks can skip a whole paired-contact group only when no current unit types can pair. The checks are repeated at group boundaries and after real contacts; skipped calls have no mutation side effects, preserving contact order and same-tick reactions of newly created products. The 128-class masks are appended after the legacy reaction lookup data; see [material table compilation](@/internals/simulation/material-table.md#chemistry-and-amount-color-buffers) and the [chemistry shader reference](@/internals/shaders/chemistry-shader.md).

## Capacity and limits

| Limit | Value |
|---|---:|
| Registered reactions | 64 |
| Records of one reaction | one per distinct set of values its materials read |
| Reactants per reaction | 1–2 |
| Products per reaction | 0–3 |
| Entries per unary or pair lookup list | 255 |
| Gas species slots per cell | 4 |
| Fluid species slots per cell | 2 |
| Queued fluid products | 4096 |

A reaction value can be a parameter of the reactants' material tags; compilation reads it for every material or pair of materials the reaction selects and stores one reaction record per distinct set of values, so lookups point each material at its own record. Material compilation rejects invalid reaction counts, ranges, amounts, or transformations, unknown reactant names or tags, and unknown product names. See [material registration](@/external-api/materials.md#reaction-properties-and-validation) and [errors](@/architecture/errors.md).

> **Limitation:** Contacts crossing the active-region edge are ignored.

> **Limitation:** A third fluid species in one cell is not represented in the species section and is invisible to chemistry.

> **Limitation:** Fluid particles consumed to amount zero remain in the pool and can still move until kill-count readback triggers compaction.

> **Limitation:** Reaction priority is registration order for each contact.

> **Limitation:** A fluid product whose particle finds the pool full is lost after its reactants were consumed.

> **Limitation:** Flame-front speed is bounded to roughly one cell per tick by contact checking; gas flow can carry flame products farther over time.

Paired-contact traversal caches which units can participate in paired reactions,
invalidating the cache before successful events can create or consume reactants.
This preserves same-tick product handling; see the
[shader reference](@/internals/shaders/chemistry-shader.md) for the cache and GPU comparisons.
