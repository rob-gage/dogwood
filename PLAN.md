# Dogwood Development Plan

Last major planning pass: 2026-09-10

## Purpose

Dogwood is a Rust/WGPU game engine for 2D worlds in which the environment is physically simulated at cellular scale.

The immediate objective is not to finish the general engine. It is to produce, within roughly one week, a coherent vertical slice that is usable for a game jam while establishing architecture that can grow into the intended engine.

This document records durable architectural intent and the long-running development plan. Short-term progress, blockers, and current tasks belong in `SPRINT.md`.

## Immediate Vertical-Slice Goal

The target demonstration should contain:

* A streamed GPU-resident cellular world.
* Static cellular materials.
* Granular cellular materials.
* Fluid particles using PBF or a closely related SPH-family method.
* A cellular/density double representation of those fluid particles for interaction with other systems.
* A controllable dummy player character.
* Basic collision and physical interaction.
* Temperature.
* At least one complete phase transition crossing representation types, preferably ice -> water and water -> ice.
* Basic force/impulse infrastructure.
* At least one destruction/material-detachment behavior.
* Basic attractive material rendering.
* Realtime 2D lighting, preferably using a minimal Radiance Cascades implementation.
* Architecture that leaves an explicit path for rigid bodies containing cellular material and interacting with all other systems.

Full generality is not required for the game-jam slice.

A working integrated subset is more valuable than several sophisticated but disconnected systems.

## Current Repository State

As of this planning pass, Dogwood already has useful infrastructure that should be preserved.

### World and streaming

`Scene` already owns:

* fixed-rate 60 Hz simulation timing;
* chunk loading/generation;
* persistent scene data;
* active and buffered tile areas;
* a two-dimensional GPU tile ring;
* asynchronous chunk streaming;
* GPU tile uploads/downloads;
* actor ownership;
* GPU cellular material storage.

Tiles are 8x8 cells.

Chunks are 64x64 tiles, or 512x512 cells.

Only the active region and its buffer are intended to remain GPU-resident. Chunks are much larger persistence/streaming units and must not become simulation units.

The actual fixed-rate `Scene::tick()` currently contains no physics simulation. Therefore most simulation behavior is greenfield even though world residency and rendering infrastructure already exist.

### Materials

The material registry already distinguishes:

* `CellularStatic`
* `CellularDynamic`
* `Fluid`

`MaterialIdentifier` stores a material form plus a form-local index.

This mechanism should remain the common material identity system unless a concrete implementation demonstrates that it cannot support a required behavior.

Material appearance is already built into GPU tables separated by material form.

Simulation-side material tables should follow the same registry rather than forming a second unrelated material system.

### Graphics

The cellular world is already rendered directly from the GPU cellular material-ID buffer.

The renderer resolves the tile-ring location for each visible world cell and looks up the appropriate form-specific material appearance.

`MaterialAppearance` already anticipates:

* freezing-point color;
* melting-point color;
* freezing-point radiance;
* melting-point radiance.

There is not yet thermal interpolation or a lighting pipeline.

### Actors

Actors already use `bevy_ecs`.

Existing infrastructure includes:

* `ActorRegistry`;
* pawns;
* possessable pawns;
* control state;
* `ScenePosition`;
* `SceneVelocity`;
* input forwarding to a possessed pawn.

This actor model should be extended rather than replaced.

## Core Development Principles

### Reuse first

Before creating anything new, inspect whether an existing Dogwood type, buffer, coordinate system, registry, ownership boundary, or module already solves part of the problem.

Existing working architecture is presumptively retained.

Do not redesign a working subsystem merely because the new subsystem could be cleaner if developed independently.

### Minimal API surface

New systems should be modular through ownership and file/module boundaries, not through speculative public APIs.

A subsystem may privately own:

* buffers;
* pipelines;
* bind groups;
* scratch state;
* shaders;
* temporary representations.

Do not create generalized infrastructure until implemented systems demonstrate a common requirement.

In particular, do not prematurely create a:

* generic compute graph;
* generic physics-field API;
* generic constraint solver;
* generic particle framework;
* generic collision-object trait;
* generic phase-transition graph;
* generic render graph;
* generic material-rule engine.

If PBF initially needs several private buffers and pipelines, PBF should own them directly.

If multiple real systems later contain clearly duplicated infrastructure, extract it then.

### Explicit authority

Whenever information exists in more than one representation, exactly one representation must be considered authoritative.

Every derived representation must have a clear regeneration/synchronization rule.

This is especially important for:

* fluid particles and fluid cellular coverage;
* future rigid-body-local cells and their world-space rasterization;
* CPU persistent chunks and active GPU simulation state.

### GPU-first active simulation

The active world should be simulated on the GPU.

CPU chunks primarily provide:

* persistence;
* streaming;
* generation;
* inactive-world storage.

The engine should avoid CPU round-trips in the normal inner simulation loop wherever possible.

### Streaming and simulation are separate concerns

Tile size and chunk size are world-management decisions.

They are not automatically appropriate workgroup sizes, spatial-hash bucket sizes, fluid support radii, lighting resolutions, or physics partition sizes.

For example, a PBF neighbor bucket should be selected from the fluid interaction radius, independently of the existing 8x8-cell tile size.

## Cellular Simulation Direction

The existing GPU cellular material-ID buffer remains the fundamental occupancy/material representation for static and dynamic cellular materials.

Additional simulation quantities should initially be represented as parallel GPU data rather than by turning every cell into a large structure.

Likely fields include, as concrete features require them:

* material identifier;
* temperature;
* cellular velocity or momentum;
* transient accumulated force;
* update/conflict state;
* derived fluid occupancy;
* future rigid-body occupancy.

Only create fields that an implemented behavior needs.

Pressure should not automatically become a persistent universal per-cell value. Fluid density, PBF lambdas, and similar quantities are solver-specific scratch state unless another system requires a derived pressure field.

## Granular Materials

Granular materials are grid-bound cellular materials.

The first implementation should prioritize:

* deterministic/safe GPU updates;
* understandable conflict handling;
* gravity;
* basic diagonal/sliding behavior;
* correct operation across tile boundaries and tile-ring wrapping.

The implementation must make explicit how multiple source cells attempting to move into the same destination are resolved.

Optimization comes after correctness and observable behavior.

## Fluid Simulation

The current preferred direction is Position Based Fluids.

A fluid particle should have an authoritative particle representation, likely containing some subset of:

* position;
* predicted position;
* velocity;
* temperature;
* material identifier;
* density/lambda/correction scratch data.

Fluid neighbor search should use its own spatial indexing based on the support radius.

The initial implementation should be decomposed into visible/understandable stages:

1. particle prediction;
2. spatial binning;
3. bucket/range construction;
4. neighbor iteration;
5. density calculation;
6. lambda calculation;
7. position correction;
8. repeated constraint iterations;
9. velocity reconstruction;
10. collision/coupling;
11. derived cellular rasterization.

Do not hide these stages behind a generalized solver abstraction before they are understood.

## Fluid Double Representation

Fluid particles remain authoritative.

A GPU pass derives a cellular representation from them.

The derived representation may eventually contain information such as:

* fluid material;
* coverage/density;
* average velocity;
* temperature;
* pressure-like information if required.

The initial representation should contain only what current interactions need.

This representation allows grid-based systems to detect/interact with fluid without forcing the PBF solver itself onto the cellular grid.

It must never become an independently simulated duplicate of the fluid.

## Temperature and Phase Changes

Temperature should initially be a deliberately simple simulation.

The first target is enough thermal behavior to demonstrate:

* heat sources;
* heat transfer;
* visible thermal/material effects;
* phase change.

The preferred first complete phase pair is:

`ice: CellularStatic -> water: Fluid`

and

`water: Fluid -> ice: CellularStatic`

Cross-form transitions are intentionally important because they test the engine's central claim that physical representations can transform into one another.

Melting requires removing cellular occupancy and spawning fluid particle state.

Freezing requires removing fluid particle state and successfully claiming cellular occupancy.

The implementation must prevent the same material/mass from existing simultaneously in both authoritative representations.

Do not build a generalized chemistry/reaction network for this milestone.

## Forces and Destruction

Introduce force/impulse concepts only as required by actual interactions.

The first destruction behavior can be simple, such as:

* breakable static material;
* impulse/force threshold;
* conversion into dynamic cellular debris.

This is sufficient to establish a transition from fixed material into mobile material.

More sophisticated fracture mechanics are deferred.

## Actors and Dummy Player

Use the existing `ActorRegistry`, possession system, `ScenePosition`, `SceneVelocity`, and control-state path.

The initial player is a debugging/game-jam character, not a generalized final character controller.

It should demonstrate:

* player input;
* gravity;
* collision with terrain;
* grounded movement;
* jumping;
* sufficient interaction with the simulated environment to validate integration.

Keep world-space actor quantities continuous. Convert into cell-space only at interaction boundaries.

The exact cellular collision strategy should be decided from the smallest solution compatible with the current GPU-authoritative world rather than by introducing a new general collision framework.

## Rigid-Body Direction

Rigid bodies must influence architectural decisions now without becoming mandatory for the first successful vertical slice.

The intended long-term representation is:

authoritative rigid body:

* transform;
* linear velocity;
* angular velocity;
* body-local cellular material;

derived interaction representation:

* world-space rasterization of body cellular material.

This mirrors the fluid strategy.

Other systems should eventually be able to inspect a combined interaction view containing:

* world cellular occupancy;
* derived fluid occupancy;
* derived rigid-body occupancy.

Contacts against a rigid body can eventually accumulate force and torque for that body.

Damage can eventually remove cells from the body-local representation and create ordinary world cellular material or particles.

Current systems must avoid designs that fundamentally prevent this model, but should not implement generalized rigid-body infrastructure until needed.

## Lighting Direction

The initial lighting target is a small GPU-only 2D lighting pipeline.

Preferred progression:

1. resolve visible material properties;
2. generate albedo/opacity/emission data;
3. generate an approximate distance field, likely using JFA or another simple GPU method;
4. compute Radiance Cascades;
5. composite lighting with material color.

Start with ordinary Radiance Cascades.

Holographic Radiance Cascades is a later optimization/experiment unless the simpler path is already complete and the sprint has sufficient remaining time.

The material rendering and lighting system should consume existing material tables rather than creating another independent appearance database.

## External Reference Strategy

The user has access to Discord communities for two in-development games/engines using similar Rust/GPU or GPU-particle technology.

The planning chat should request references when observed behavior or developer comments could materially resolve uncertainty.

High-value reference gates include:

* granular GPU concurrency/update scheduling;
* fluid-cellular double representation;
* two-way fluid/granular interaction;
* fluid collision against cellular terrain;
* rigid bodies composed of cellular material;
* rasterization of moving rigid cellular bodies;
* destruction/detachment behavior;
* temperature/phase transitions;
* Radiance Cascades implementation details.

Requests should be narrow.

Prefer:

“Send the clearest clip of a rigid cellular object breaking apart and any nearby developer explanation of how its pixels are represented.”

over:

“Send all their technical information.”

Videos and pictures are useful evidence even without source code because observable artifacts can constrain likely implementation choices.

## Understanding Requirement

No major subsystem is considered fully integrated from the user's perspective until the user can explain:

* which representation is authoritative;
* where its important buffers/state live;
* how world coordinates map to its data;
* which passes execute and in what order;
* how it synchronizes with adjacent systems;
* one important concurrency or correctness hazard;
* one known limitation.

Understanding checks happen in planning/review chats, not autonomously inside Codex.

Codex should provide enough implementation information for those checks but may not mark them complete.

## Codex Strategy

Codex is primarily an implementation multiplier.

Architecture should normally be settled in planning chat before a substantial Codex task is issued.

Each Codex prompt should define a bounded, independently reviewable objective and specify:

* what existing infrastructure must be reused;
* what files/systems are in scope;
* what is explicitly out of scope;
* whether any new public API is permitted;
* authoritative data ownership;
* required observable acceptance result;
* required handoff information.

Large features should be split by understandable stages rather than assigned as monolithic subsystem implementations.

### Codex sessions

One Codex session has one coherent objective or tightly coupled sequence of objectives.

A session uses one model from beginning to end.

Do not switch the model within an active session.

When a planning chat recommends Codex work, it should explicitly state:

`Codex session: NEW` or `Codex session: CONTINUE`

`Model: <exact model>`

`Objective: <one coherent objective>`

`Stop when: <specific acceptance condition>`

Continue a session for corrections, debugging, or small follow-ups required to complete its original objective.

Start a new session when:

* beginning another subsystem;
* beginning an independently reviewable implementation slice;
* the required model tier changes;
* a difficult design phase is finished and remaining work can use a cheaper model;
* the existing session has accumulated unrelated completed context;
* the next task would materially expand the objective.

Stop the session when:

* the objective works;
* the acceptance criterion has been exercised;
* the implementation handoff is complete;
* remaining work is meaningfully a different objective.

### Model selection

Use the least expensive model that is appropriate.

Current working ladder:

**GPT-5.6 Luna**

Use for very small mechanical tasks with an explicit design, documentation edits, simple repetitive changes, and trivial cleanup.

**GPT-5.6 Terra**

Use for routine contained implementation where architecture and algorithm are already decided: straightforward Rust plumbing, WGPU boilerplate, simple test/demo work, and uncomplicated integration.

**GPT-5.6 Sol**

Default model for substantial Dogwood implementation: nontrivial Rust/WGSL work, GPU algorithms, synchronization, multi-file integration, and implementation where significant reasoning remains.

**GPT-6 Astra**

Reserve for the hardest work: novel cross-system architecture, difficult GPU concurrency/correctness problems, ambiguous coupling design, hard debugging, or cases where Sol cannot resolve the problem reliably.

Do not burn Astra on mechanical implementation after Astra/Sol has already settled the design.

Model availability may change. When opening a session, the planning chat should name the exact currently available model rather than relying only on this file.

## Git Ownership

The user owns Git operations and repository history.

Codex must not:

* stage files;
* create commits;
* amend commits;
* create/delete/switch branches;
* merge;
* rebase;
* reset;
* push;
* pull;
* tag;
* otherwise manipulate Git state.

Do not invoke Git commands unless the user explicitly asks for a read-only Git inspection.

Codex finishes with modified working-tree files and a precise handoff.

The user reviews the changes and decides what to commit.

This is intentional: manual Git review is part of understanding the engine.

## Approximate Implementation Sequence

The ordering is dependency-driven rather than rigidly calendar-driven.

### Phase 0 — Sprint control and architecture

Establish:

* `PLAN.md`;
* `AGENTS.md`;
* `SPRINT.md`;
* representation ownership;
* task/session discipline;
* understanding workflow.

### Phase 1 — First actual GPU simulation

Turn the currently empty physics tick into the smallest real GPU simulation path.

Do not begin by designing a general compute framework.

Add only the private WGPU machinery required by the first simulation task.

### Phase 2 — Granular cellular behavior

Get a dynamic cellular material visibly moving safely.

Acceptance example: sand falls, settles, and crosses tile boundaries correctly.

### Phase 3 — Dummy player

Use existing actor/input infrastructure to create a controllable pawn with minimal terrain collision and movement.

### Phase 4 — PBF

Build the particle representation and PBF stages incrementally.

Standalone water behavior must work before complicated coupling is added.

### Phase 5 — Fluid double representation and coupling

Rasterize fluid state into cellular interaction data.

Demonstrate water interacting coherently with cellular terrain and at least basic granular material.

### Phase 6 — Thermal state and transformations

Add enough temperature behavior to demonstrate ice/water phase changes.

### Phase 7 — Force/destruction

Demonstrate at least one force-driven material transition or destruction behavior.

### Phase 8 — Lighting and appearance

Add material appearance improvements and a minimal Radiance Cascades path.

### Phase 9 — Rigid-body integration path

If the critical vertical slice is stable, implement or prototype the body-local + world-rasterized rigid cellular path.

If schedule pressure is high, architectural compatibility is sufficient and complete rigid-body dynamics are deferred.

### Phase 10 — Stabilization

Stop adding systems.

Profile, fix correctness issues, create debugging visualizations, and produce the game-jam-ready demo.

## Scope-Cut Order

If the schedule slips, cut complexity before cutting the central interaction experiment.

Cut or defer approximately in this order:

1. Holographic Radiance Cascades.
2. Sophisticated/general rigid-body dynamics.
3. General fracture.
4. Persistent fluid simulation outside the active region.
5. Elaborate thermodynamics.
6. Complex chemistry/reactions.
7. Advanced appearance variation.
8. Advanced character movement.
9. Multi-material generalized phase-transition systems.

Do not cut the basic fluid double representation if avoidable.

That representation is one of the most important architectural experiments in Dogwood.
