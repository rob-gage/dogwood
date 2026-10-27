+++
title = "Thermal simulation"
description = "Cell heat storage, conduction, ambient exchange, and fluid coupling."
weight = 50
[extra]
sources = ["engine_world/src/scenes/thermal", "engine_world/src/scenes/scene.rs", "engine_world/src/scenes/fluids", "engine_world/src/scenes/pressure", "engine_world/src/materials"]
+++

Thermal simulation stores temperature and amount per cellular cell. Material heat capacity converts these values into heat; material conduction controls exchange with neighboring cells. Fluid particles carry their own temperature, amount, and color, and contribute heat to their occupied cells.

Each thermal step processes awake tiles. Heat transfer is accumulated in fixed-point units, applied to matter, and shared with fluids and gas. Thermal sleeping flags allow unchanged tiles to skip work; edits, collisions, fluid motion, and heat exchange wake affected tiles. Exposed gas cells relax toward the scene's ambient gas temperature and conduction.

Material thermal properties include hot and cold transition temperatures, latent heat, product
material, amount, and color transform. The material table retains these properties, but transitions
are not simulated. Gas temperature changes through thermal conduction; pressure transport does not carry heat.

The [scene pipeline](@/architecture/scene-pipeline.md) gives the exact tick order. GPU bindings and records are in the [thermal shader reference](@/internals/shaders/thermal-shader.md).
