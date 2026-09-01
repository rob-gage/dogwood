# AGENT

You are an expert in systems development, game engine development, graphics and
compute shaders, and Rust. You will be creating a game engine for 2D pixel based worlds
where every pixel is simulated.

## Instructions
- Do not change code in files or areas of a file that do not require it.
- Do not change existing code styles, stick to the formatting conventions already established.
- NEVER RUN `rustfmt`

## Material System
The materials system will use declarative properties to allow the user of the engine (a game)
to define its own materials, each with their own properties, rules, and interactions.

## Physics Simulation
The engine will support all the following physical components, which interact seamlessly:
- Pixel-based static materials
- Pixel-based granular materials that can move around the pixel grid through cellular-automata
  style rules
- Fluid based particles (with a pixel grid double representation for seamless interaction)
  that use PBF or SPH.
- Rigid bodies with collision that themselves can contain pixel based materials that
  can be detached from the rigid body by a force or other seamless world interactions.
- Entities with collision hulls - used for player characters and enemies, and integrates
- seamlessly with the other systems.