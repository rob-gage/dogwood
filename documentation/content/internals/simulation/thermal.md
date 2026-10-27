+++
title = "Thermal simulation"
description = "Cell heat storage, conduction, ambient exchange, phase transitions, and fluid coupling."
weight = 50
[extra]
sources = ["engine_world/src/scenes/thermal", "engine_world/src/scenes/scene.rs", "engine_world/src/scenes/fluids", "engine_world/src/scenes/pressure", "engine_world/src/materials", "engine_graphics/src/renders/scene_renderer_cellular_pass.rs", "engine_graphics/src/renders/scene_renderer_fluid_pass.rs", "engine_graphics/src/renders/scene_renderer_gas_pass.rs", "engine_graphics/src/renders/scene_renderer_lighting_pass.rs", "engine_graphics/src/renders/scene_renderer_cellular.wgsl", "engine_graphics/src/renders/scene_renderer_fluid.wgsl", "engine_graphics/src/renders/scene_renderer_gas.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_optical.wgsl"]
+++

Thermal simulation stores temperature and amount per cellular cell. Cellular cells, fluid particles, and fluid rasters carry packed radiance with their color; transitions transform both values. Material heat capacity converts these values into heat; material conduction controls exchange with neighboring cells. Fluid particles carry their own temperature, amount, and color, and contribute heat to their occupied cells.

Each thermal step processes awake tiles. Heat transfer is accumulated in fixed-point units, applied to matter, and shared with fluids and gas. Thermal sleeping flags allow unchanged tiles to skip work; edits, collisions, fluid motion, and heat exchange wake affected tiles. Exposed gas cells relax toward the scene's ambient gas temperature and conduction.

## Transitions and latent heat

Materials can define hot and cold transitions with a transition temperature, latent heat, product material, amount, and color and radiance transforms. Transitions apply both transforms to carried values. Gas transitions are due when cell temperature reaches the transition temperature; their product temperature uses sensible heat only, and their latent heat is added to or removed from the cell. Cellular matter and fluid particles hold the transition temperature while latent heat banks, and their product temperature accounts for that latent heat. The thermal apply pass converts cellular and gas transitions where possible and records fluid conversions for the fluid pass.

Fluid particles due for transition are converted before compaction. Thermal lists cells and gas slots that become fluid; `convert_cells` reserves particles and clears the source, while `convert_particles` changes existing particle material, amount, temperature, and color. When a conversion would create a static cell with no static material in its four side-neighbors, it uses the product's configured debris material as a dynamic cell when one exists. Thermal conversions place that debris at rest; fluid conversions carry the particle velocity. Both keep the carried color and transformed radiance. Products without debris remain static. Fluid particles then receive the cell temperature delta from the thermal step. Pressure transport carries gas heat with moved gas.

> **Limitation:** Pressure flow is not temperature-dependent. Condensation leaves residual vapor below one particle's worth. Conversions can wait when the fluid pool or gas slots are full. Collision sees converted terrain one tick later.

Temperature-color keys are a render-time appearance transform: neighboring keys interpolate by temperature, values outside the range use the nearest key, and no keys means identity. They do not modify stored color or radiance. Fluid keys apply only where the fluid heat raster has heat; see [material table compilation](@/internals/simulation/material-table.md).

> **Limitation:** Fluid temperature colors are applied only where the fluid heat raster has heat.

The [scene pipeline](@/architecture/scene-pipeline.md) gives the exact tick order. GPU bindings and records are in the [thermal shader reference](@/internals/shaders/thermal-shader.md).
