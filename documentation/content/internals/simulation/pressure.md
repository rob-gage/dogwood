+++
title = "Pressure simulation"
description = "How pressure occupies cellular tiles, flows through open and solid cells, damages materials, and is published to consumers."
weight = 78
[extra]
sources = ["engine_world/src/scenes/pressure", "engine_world/src/scenes/scene.rs", "engine_world/src/scenes/cellular_tiles/cellular_tile_buffers.rs", "engine_world/src/scenes/edits/edit_cellular_tile.wgsl", "engine_world/src/materials/material_table.rs"]
+++

`Pressure` owns one pressure substrate over every cell in the active cellular tile region. It runs after cellular matter, collision, and fluids have submitted their work. The [pressure shader](@/internals/shaders/pressure-shader.md) documents bindings and packed records; [scene update order](@/architecture/scene-pipeline.md#simulation-order) places it in the full tick.

## Cell state and units

Each cell has four gas species slots, phantom air, right and top staggered face flows, and pressure state. Species slots pack an 8-bit gas index and 24-bit amount. `GAS_AMOUNT_PER_ATMOSPHERE` is 4096 amount units. Each slot is limited to `0xff_ffff`; givers leave enough room for receivers, so receiver-side clamping is not used. `PRESSURE_MAXIMUM` is 4095 atmospheres per cell.

Open cells contain species and phantom air. Pressure is total amount divided by measured open volume; gas mass also contributes to density and buoyancy. Static terrain and rasterized rigid-body cells are solid. Their signed stored pressure is an `i32` gauge amount in gas word 7; absolute pressure is ambient plus stored pressure. They transmit pressure to open cells and to solid cells with the same owner (terrain or the same rigid body), with mass-based inertia and impedance. Solid faces absorb and relax pressure, and solids expel trapped gas through open faces. Faces between different solid owners reflect.

Dynamic cells, fluid-filled cells, actor-covered cells, and other closed cells reflect pressure. Gas trapped in a closed cell is pushed out through open faces when a route exists. Gas species without phantom air do not enter solids.

## Flows, volume, and sleeping

Face flows are staggered and double buffered. Pressure waves and bulk viscosity update the flows; transfers are computed from the prior state and split between species and phantom air. Phantom air diffuses separately. Material gas `diffusivity` is a per-tick fraction in `[0, 0.4]`; the template water vapor value is `0.04`. Gravity drives buoyancy below pressure-wave speeds. Moving actors drag nearby gas flow.

The pass measures open volume from cellular occupancy, fluid raster occupancy, and actor coverage. It measures the full ring when the active region or ring offset changes, then revisits tiles marked volume-dependent. Collection, clearing, and preparation build a sparse active-tile dispatch. Changed volume, pressure, or nonzero face flow wakes nearby tiles. Flow below its minimum becomes zero, allowing settled tiles to sleep. A flow reaching a sleeping neighbor wakes it for a later tick.

Each tick has 12 fixed substeps (`PRESSURE_SUBSTEP_COUNT`) over `PRESSURE_TIME_STEP = 1/60 s`. The pass derives its per-substep coefficients from these per-second constants:

| Constant | Value | Meaning |
| --- | ---: | --- |
| `PRESSURE_BUOYANCY_SCALE` | 0.5 | Fraction of scene gravity applied to gas |
| `PRESSURE_GAS_SPEED` | 18 tiles/s | Pressure-wave speed through gas |
| `PRESSURE_SOLID_SPEED` | 54 tiles/s | Pressure-wave speed through solids |
| `PRESSURE_SOLID_IMPEDANCE` | 5.0 | Solid impedance relative to gas for unit mass |
| `PRESSURE_FLOW_DAMPING` | 0.96 | Face flow removed per second |
| `PRESSURE_BULK_VISCOSITY` | 24.0 | Net-flow divergence response per second |
| `PRESSURE_FLOW_MINIMUM` | 360 amount units/s | Flow cutoff for sleeping |
| `PRESSURE_AMBIENT_DIFFUSIVITY` | 4.8 | Phantom-air exchange rate per second |
| `PRESSURE_ACTOR_DRAG` | 53.5 | Flow response rate to actor motion |
| `PRESSURE_SOLID_RELAXATION` | 2.0 | Stored-pressure relaxation rate per second |
| `PRESSURE_TRAPPED_EXPULSION` | 8.0 | Trapped-gas expulsion rate per second |
| `PRESSURE_DAMAGE_RATE` | 40.0 | Integrity loss per second per excess atmosphere and absorption |

The GPU storage also uses a `VOLUME_SCALE` of 65,536 open-volume units, a `PRESSURE_IMPULSE` of 0.1111 mass·cells/tick per atmosphere per face, and a 0.02-atmosphere minimum to suppress tiny pushes.

## Solid integrity and damage

Solid material properties provide static `strength` in atmospheres and `absorption` in `[0, 1]`. Template stone uses strength `10` and absorption `0.1`; positive infinity makes a material unbreakable. The low 16 bits of `kinematics.y` store integrity, with zero meaning intact. Each pressure substep measures load as the maximum absolute stored pressure or gauge pressure among open neighbors, absorbs pressure, and reduces integrity above material strength.

After publishing pressure, `damage` breaks cells whose integrity is exhausted. A broken cell becomes its configured dynamic debris material, retaining color and receiving velocity from the pressure push, or becomes empty. Stored pressure is released as phantom air. Rigid-body cells are removed from their body's cell mask and reported for later topology updates; broken tiles are reported with their remaining static occupancy.

## Published pressure and consumers

`publish` writes `CellularTileBuffers::pressure_buffer` in gauge atmospheres: open cells publish gas pressure, solids publish stored pressure, and other closed cells publish trapped amount. It also wakes dynamic cells next to a pushing pressure. Consumers use the published buffer from the previous tick:

- Dynamic cells read pressure in `integrate`.
- Actors read pressure in `contact`.
- Rigid bodies read pressure on open faces in `rigid_fluid` for force and torque.
- Fluid particles read pressure during the first `predict` substep.

Each face push uses `PRESSURE_IMPULSE` and the 0.02-atmosphere minimum. See [cellular kinematics](@/internals/simulation/cellular-kinematics.md), [contact exchange](@/internals/simulation/contact-exchange.md), and [fluid coupling](@/internals/simulation/fluid-coupling.md) for consumer behavior.

Active-region edges act as walls: gas never leaves the region, so pressure from explosions and pressure edits stays and builds up inside it, and waves reflect at the edges.

> **Limitation:** The model is isothermal and has no thermal simulation. Momentum is not exactly conserved across the substrate and its consumers. Faces between terrain and a body, or between different bodies, reflect. Contact impulses do not yet load the pressure substrate.

> **Limitation:** Pure species gas without phantom air does not enter solids. Static occupancy refresh on region/ring movement and edits, edit changed-cell reads, saves, and streaming still use blocking occupancy reads. Pressure breakage feedback itself is asynchronous and applied incrementally; see [collision ordering](@/internals/simulation/collision-world.md).
