+++
title = "Pressure simulation"
description = "How pressure occupies cellular tiles, flows through open and solid cells, damages materials, and is published to consumers."
weight = 78
[extra]
sources = ["engine_world/src/scenes/pressure", "engine_world/src/scenes/scene.rs", "engine_world/src/scenes/cellular_tiles/cellular_tile_buffers.rs", "engine_world/src/scenes/edits/edit_cellular_tile.wgsl", "engine_world/src/materials/material_table.rs"]
+++

`Pressure` owns one pressure substrate over every cell in the active cellular tile region. It runs after cellular matter, collision, and fluids have submitted their work. The cells keep their gas state, but each tick simulates it on coarser [pressure cells](#pressure-cells) of 2×2 cells. The [pressure shader](@/internals/shaders/pressure-shader.md) documents bindings and packed records; [scene update order](@/architecture/scene-pipeline.md#simulation-order) places it in the full tick.

## Cell state and units

Each cell has four gas species slots, phantom air, right and top staggered face flows, and pressure state. Species slots pack an 8-bit gas index and 24-bit amount. `GAS_AMOUNT_PER_ATMOSPHERE` is 4096 amount units. Each slot is limited to `0xff_ffff`; givers leave enough room for receivers, so receiver-side clamping is not used. `PRESSURE_MAXIMUM` is 4095 atmospheres per cell.

Open cells contain species and phantom air. Pressure is total amount divided by measured open volume; gas mass also contributes to density and buoyancy. Static terrain and rasterized rigid-body cells are solid. Their signed stored pressure is an `i32` gauge amount in gas word 7; absolute pressure is ambient plus stored pressure. They transmit pressure to open cells and to solid cells with the same owner (terrain or the same rigid body), with mass-based inertia and impedance. Solid faces absorb and relax pressure, and solids expel trapped gas through open faces. Faces between different solid owners reflect.

Dynamic cells, actor-covered cells, and other closed cells reflect pressure. Fluid occupancy above `FLUID_OCCUPANCY_CLOSED` reduces a cell's gas volume to the unoccupied fraction, with a minimum of `FLUID_GAS_PERMEABILITY`; this lets gas bubble through fluid while its pressure pushes fluid particles aside. Gas trapped in a closed cell is pushed out through open faces when a route exists. Gas species without phantom air do not enter solids.

## Pressure cells

A pressure cell covers 2×2 cells, and a pressure tile of 8×8 pressure cells covers a block of 2×2 cellular tiles. At the start of each tick, `coarsen` rebuilds the pressure cells that the listed blocks' halos reach:

- Phantom air and gas species are summed, merged into at most four species.
- Open volume is the mean of the cells' open volumes, and drag is the mean drag of the dragged cells.
- Face flows are those of the bottom-left cell.
- A pressure cell whose four cells are all static is solid. It sums their stored pressure, averages their density, and takes the first cell's owner and absorption.
- A pressure cell holding some static cells but not only static cells is partial: it is open, and its gas presses on those static cells.

The substeps then simulate the pressure cells with the same flow, transfer, and solid model as cells, using coefficients rescaled for the larger cell. After the substeps, `refine` writes the result back over the cells of the active tiles:

- Phantom air and each species are split among cells by open volume, or evenly when none is open. Fluid-occupied cells retain partial volume, so refinement does not distribute bubbling gas evenly into a fully closed 2×2 group. Rounding remainders go to the most open cell, so no gas is lost.
- Every cell keeps its pressure cell's face flows.
- A static cell in a solid pressure cell takes an equal share of its stored pressure, so it holds the same pressure in atmospheres. One in a partial pressure cell stores the gauge pressure of the gas pressing on it.

Edits, streaming, rendering, and saves see only cells.

> **Limitation:** A wall one cell thick shares pressure cells with the open cells beside it, so pressure leaks through it. A fifth gas species in one pressure cell is lost when it is rebuilt. Pressure detail smaller than a pressure cell averages out, so a broken cell's released pressure throws its debris less than it would at cell resolution.

## Flows, volume, and sleeping

Face flows are staggered and double buffered. Pressure waves and bulk viscosity update the flows; transfers are computed from the prior state and split between species and phantom air. Phantom air diffuses separately. Material gas `diffusivity` is a per-tick fraction in `[0, 0.4]`; the template water vapor value is `0.04`. Gravity drives buoyancy below pressure-wave speeds. Moving actors drag nearby gas flow.

The pass measures open volume per cell from cellular occupancy, fluid raster occupancy, and actor coverage. It measures the full ring when the active region or ring offset changes, then revisits tiles marked volume-dependent. Collection and clearing list the awake tiles and their neighbors as active, and the step runs one workgroup per block of 2×2 tiles holding an active tile.

A tile stays awake while one of its pressure cells changes a species slot, changes its phantom air by more than 64 amount units in a substep, changes its stored pressure, relaxes, or keeps a face flow of at least 16 amount units. Smaller changes let the tile sleep. Flow below its minimum becomes zero. A flow reaching a sleeping neighbor wakes it for a later tick.

Gas pressure relaxes toward the ambient pressure. Each substep, a pressure cell more than 0.01 atm from the ambient pressure moves toward it by `PRESSURE_OPEN_RELAXATION` of the difference beyond that tolerance, and by at least `PRESSURE_OPEN_RELAXATION_MINIMUM`. Excess pressure removes phantom air first, then all species in proportion. A deficit adds phantom air. A closed pressure cell only loses the overpressure of the gas it traps. Sealed pockets and the low pressure behind a passing wave therefore settle within about a second.

Each tick has 3 fixed substeps (`PRESSURE_SUBSTEP_COUNT`) over `PRESSURE_TIME_STEP = 1/60 s`. The pass derives its per-substep coefficients from these per-second constants, converting tiles to pressure cells:

| Constant | Value | Meaning |
| --- | ---: | --- |
| `PRESSURE_BUOYANCY_SCALE` | 0.5 | Fraction of scene gravity applied to gas |
| `PRESSURE_GAS_SPEED` | 18 tiles/s | Pressure-wave speed through gas |
| `PRESSURE_SOLID_SPEED` | 30 tiles/s | Pressure-wave speed through solids |
| `PRESSURE_SOLID_IMPEDANCE` | 5.0 | Solid impedance relative to gas for unit mass |
| `PRESSURE_FLOW_DAMPING` | 0.96 | Face flow removed per second |
| `PRESSURE_BULK_VISCOSITY` | 24.0 | Net-flow divergence response per second |
| `PRESSURE_FLOW_MINIMUM` | 360 amount units/s | Flow cutoff for sleeping |
| `PRESSURE_AMBIENT_DIFFUSIVITY` | 4.8 | Phantom-air exchange rate per second |
| `PRESSURE_ACTOR_DRAG` | 53.5 | Flow response rate to actor motion |
| `PRESSURE_SOLID_RELAXATION` | 2.0 | Stored-pressure relaxation rate per second |
| `PRESSURE_OPEN_RELAXATION` | 3.0 | Gas-pressure relaxation rate per second |
| `PRESSURE_OPEN_RELAXATION_MINIMUM` | 0.5 atm/s | Smallest gas-pressure relaxation |
| `PRESSURE_TRAPPED_EXPULSION` | 8.0 | Trapped-gas expulsion rate per second |
| `PRESSURE_DAMAGE_RATE` | 40.0 | Integrity loss per second per excess atmosphere and absorption |

The GPU storage also uses a `VOLUME_SCALE` of 65,536 open-volume units, a `PRESSURE_IMPULSE` of 0.1111 mass·cells/tick per atmosphere per face, and a 0.02-atmosphere minimum to suppress tiny pushes.

## Solid integrity and damage

Solid material properties provide static `strength` in atmospheres and `absorption` in `[0, 1]`. Template stone uses strength `10` and absorption `0.1`; positive infinity makes a material unbreakable. The low 16 bits of `kinematics.y` store integrity, with zero meaning intact. Solid faces absorb pressure as it passes. Each substep, a solid or partial pressure cell measures its load in atmospheres:

- A solid pressure cell's load is the largest of its absolute stored pressure and the gauge pressure of its open neighbors.
- A partial pressure cell's load is its own gauge pressure.

Each static cell under that pressure cell loses integrity while the load exceeds its material's strength.

After publishing pressure, `damage` breaks cells whose integrity is exhausted. A broken cell becomes its configured dynamic debris material, retaining color and radiance and receiving velocity from the pressure push, or becomes empty. Stored pressure is released as phantom air. Rigid-body cells are removed from their body's cell mask, and changed tiles are reported with their remaining static occupancy for later topology updates.

## Published pressure and consumers

`publish` writes `CellularTileBuffers::pressure_buffer` in gauge atmospheres from the newest pressure cells:

- Static cells publish their stored pressure.
- Other cells publish the gauge pressure of the open pressure cells around them, interpolated bilinearly so pushes stay even across pressure cell edges.
- Without an open pressure cell nearby, a cell publishes its own pressure cell's value. For a closed pressure cell, that is its trapped gas above the ambient pressure, never a vacuum.

`publish` also wakes dynamic cells next to a pushing pressure. Consumers use the published buffer from the previous tick:

- Dynamic cells read pressure in `integrate`.
- Actors read pressure in `contact`.
- Rigid bodies read pressure on open faces in `rigid_fluid` for force and torque.
- Fluid particles read pressure during the first `predict` substep.

Each face push uses `PRESSURE_IMPULSE` and the 0.02-atmosphere minimum. See [cellular kinematics](@/internals/simulation/cellular-kinematics.md), [contact exchange](@/internals/simulation/contact-exchange.md), and [fluid coupling](@/internals/simulation/fluid-coupling.md) for consumer behavior.

Active-region edges act as walls: gas never leaves the region, so pressure from explosions and pressure edits stays and builds up inside it, and waves reflect at the edges.

Gas pressure carries heat: gas amount and material heat capacity determine its thermal contribution, and pressure transport moves gas heat between cells.

> **Limitation:** Pressure flow is not temperature-dependent. Momentum is not exactly conserved across the substrate and its consumers. Faces between terrain and a body, or between different bodies, reflect. Contact impulses do not yet load the pressure substrate.

> **Limitation:** Pure species gas without phantom air does not enter solids. Static occupancy refresh on region/ring movement and edits, edit changed-cell reads, saves, and streaming still use blocking occupancy reads. Collision-owned changed-cell feedback is asynchronous and applied incrementally; see [collision ordering](@/internals/simulation/collision-world.md).
