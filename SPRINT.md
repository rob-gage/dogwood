# Dogwood Vertical-Slice Sprint

Sprint initialized: 2026-09-10

Target: working game-jam-ready engine vertical slice within approximately seven days.

This file is deliberately volatile.

Update it as implementation reality changes.

`PLAN.md` remains the durable architecture document.

## Definition of Success

The sprint succeeds when Dogwood can run one coherent demonstration in which most of the following happen in the same world:

* static cellular terrain renders;
* granular material moves;
* PBF/SPH-family fluid particles move;
* fluid is represented to the cellular interaction system;
* a possessed dummy player can move and collide;
* multiple material types coexist;
* temperature exists;
* at least one phase transition crosses physical representations;
* at least one force/destruction interaction exists;
* the scene has basic attractive material appearance;
* realtime 2D lighting works;
* future rigid cellular bodies have not been architecturally blocked.

A stable subset is preferable to nominally checking every item with broken integration.

## Current Baseline

### Working

* Rust workspace split into engine-related crates.
* WGPU `Accelerator`.
* GPU storage-buffer allocation.
* `Scene`.
* 60 Hz fixed simulation scheduling.
* Chunk persistence/generation.
* Asynchronous chunk streaming.
* 8x8-cell tile representation.
* 64x64-tile chunks.
* GPU-resident active/buffered tile ring.
* Cellular material-ID uploads/downloads.
* Static cellular world generation.
* Cellular rendering from the GPU material-ID buffer.
* Form-specific GPU material appearance tables.
* `CellularStatic`, `CellularDynamic`, and `Fluid` material forms.
* Form-tagged `MaterialIdentifier`.
* `bevy_ecs` actor registry.
* Pawn/possessable pawn infrastructure.
* Actor position and velocity.
* Input/control-state routing to a possessed actor.
* Camera can follow the possessed actor.

### Missing or essentially unimplemented

* Actual contents of the physics tick.
* Granular cellular movement.
* Fluid particle storage.
* PBF/SPH simulation.
* Fluid spatial bucketing.
* Fluid-cellular double representation.
* Cellular-fluid coupling.
* Actor collision/movement physics.
* Temperature simulation.
* Phase transitions.
* Force/impulse infrastructure.
* Destruction.
* Rigid bodies.
* Rigid-body cellular rasterization.
* Lighting.
* Thermal material rendering.
* Significant automated simulation validation.

## Known Small Defect

`MaterialAppearance::with_radiance()` currently appears to assign `radiance_freezing` twice rather than assigning the second value to `radiance_melting`.

Fix opportunistically when working in the relevant area or as a tiny isolated task.

## Sprint Rules

* Existing infrastructure wins unless it creates a demonstrated blocker.
* No speculative framework building.
* No unnecessary public API.
* No unrelated refactoring.
* No general-purpose system before its concrete behavior exists.
* Every substantial task gets an observable acceptance case.
* Every substantial task gets an understanding check.
* User performs all Git operations and commits.
* Codex never marks human understanding complete.
* Stop adding features when stabilization becomes more valuable than another subsystem.

## Architecture Decisions Currently Accepted

### Active world

GPU state is authoritative for active cellular simulation.

Chunks remain persistence/streaming storage.

### Materials

Continue using `MaterialIdentifier` and current material forms.

Simulation properties should derive from the existing registry.

### Fluids

Initial preferred solver: PBF.

Fluid particle state is authoritative.

Cellular fluid data is derived.

Fluid spatial buckets are independent of the 8x8 tile size.

### Rigid bodies

Long-term preferred representation:

body-local cellular data + transform/velocity as authoritative;

world-space cellular rasterization as derived interaction data.

Full rigid-body dynamics are not on the initial critical path.

### Lighting

Implement ordinary Radiance Cascades before experimenting with Holographic Radiance Cascades unless sprint conditions change.

### API philosophy

Prefer private subsystem implementation.

Generalize only after multiple real consumers prove the abstraction.

## Decisions Still Requiring Focused Design

These should be resolved immediately before their implementation rather than guessed now.

### Granular update conflict algorithm

Need a GPU-safe movement/conflict strategy.

Possible external-reference gate: request demonstrations or technical comments about update scheduling from the similar engines.

### Player/cellular collision path

Need the smallest strategy compatible with the GPU-authoritative cellular world and current CPU actor ECS.

Avoid designing a general collision framework prematurely.

### Exact fluid-cellular raster format

Determine the minimum fields needed for first two-way interactions.

Do not add pressure/velocity/etc. merely because they might eventually be useful.

Possible external-reference gate: request fluid/granular and fluid/rigid interaction demonstrations.

### Temperature representation/persistence

Determine minimum active GPU representation.

Default assumption: temperature does not need full persistent-world support during the first sprint unless demonstrated otherwise.

### Cross-form transition allocation

Need a safe mechanism for:

* cellular -> particle spawning;
* particle -> cellular claiming.

### Rigid interaction overlay

Design only when an actual rigid-body task is reached unless an earlier interaction system requires the interface.

## Execution Queue

Priorities are dependency ordered.

### 0. Planning/control files

Goal:

Create and manually review:

* `PLAN.md`
* `AGENTS.md`
* `SPRINT.md`

Acceptance:

User has reviewed and committed them manually.

Understanding check:

User can explain the distinct role of all three documents.

### 1. First GPU simulation slice

Goal:

Turn the empty `Scene::tick()` into an actual simulation path without creating a generalized compute framework.

Work should establish only the minimum WGPU resources needed by the first cellular simulation.

Acceptance:

A compute pass visibly changes controlled simulation state in the actual active tile-ring representation.

Understanding check should cover:

* how logical world cells map into physical ring-buffer storage;
* active versus buffered tiles;
* where the compute dispatch is encoded/submitted;
* why the design is not yet a generic compute framework.

Recommended Codex session:

`NEW`

Model: **GPT-5.6 Sol**

Reason: first real GPU simulation path establishes patterns that later systems will depend on.

Stop when:

The first actual compute-driven active-world change works and has been reviewed.

### 2. Granular material

Goal:

Implement minimal dynamic cellular/granular movement.

First material: sand or equivalent.

Acceptance:

A pile/column falls, resolves conflicts safely, settles, and behaves correctly across relevant tile boundaries.

Understanding check must cover:

* movement proposal;
* destination conflicts;
* workgroup/race safety;
* tile-ring indexing;
* why the chosen algorithm is safe enough.

Recommended Codex session:

Normally `NEW`.

Model: **GPT-5.6 Sol**

If Task 1 directly created the private granular pipeline and Task 2 is merely completing the same coherent implementation, continuing that Sol session may be appropriate.

### 3. Dummy player

Goal:

Spawn and possess a pawn and consume existing control state to provide basic movement/collision.

Reuse:

* `ActorRegistry`;
* `ActorPawn`;
* `ActorPossessable`;
* `ActorControlState`;
* `ScenePosition`;
* `SceneVelocity`;
* existing input translation.

Acceptance:

Player can move, remain outside static terrain, become grounded, and jump.

Understanding check must cover:

* CPU/GPU ownership;
* collision information path;
* unit conversion between tile/world quantities and cells;
* integration order relative to world simulation.

Codex model:

**Terra** if the collision strategy has already been completely designed and work is mostly contained plumbing.

**Sol** if GPU/CPU collision synchronization or collision representation still requires substantive reasoning.

Start a new session if the model or objective differs from the granular session.

### 4. Fluid particle storage and spatial indexing

Goal:

Create the minimal authoritative fluid-particle state and neighbor-search structure.

Do not implement every PBF stage at once.

Acceptance:

Particles can be spawned and spatial buckets/ranges can be visualized or otherwise validated.

Understanding check must cover:

* particle layout;
* support radius;
* bucket size;
* bucket coordinate derivation;
* neighboring bucket iteration;
* buffer construction.

Codex session:

`NEW`

Model: **GPT-5.6 Sol**

Stop when particle storage and validated neighbor lookup work.

Do not carry unrelated cellular work into this session.

### 5. PBF solver

Goal:

Add PBF stages incrementally over validated particle/bucket infrastructure.

Acceptance:

A water body falls, pools, remains reasonably stable/incompressible, and collides with basic terrain.

Understanding check must cover:

* prediction;
* density constraint;
* lambda;
* correction;
* solver iterations;
* velocity reconstruction;
* relevant stability parameters.

Codex session:

Usually `NEW` after the spatial-index task has been reviewed.

Model: **GPT-5.6 Sol**

Escalate into a new **Astra** session only if a difficult solver correctness/concurrency problem cannot be resolved cleanly with Sol.

### 6. Fluid cellular double representation

Goal:

Rasterize fluid particles into the minimum cellular interaction representation.

Acceptance:

Derived cellular data tracks authoritative particles and is usable by another system.

Understanding check must cover:

* which representation is authoritative;
* how rasterization works;
* what information is intentionally discarded;
* update timing;
* how stale/double occupancy is prevented.

Codex session:

`NEW`

Model: **GPT-5.6 Sol**

This is a likely external-reference gate.

Request a specific reference before implementation if similar-engine evidence can resolve uncertainty about coupling/rasterization.

### 7. Granular-fluid coupling

Goal:

Allow sand and water to coexist and influence one another enough for the vertical slice.

Acceptance:

An observable test scene demonstrates coherent contact/displacement rather than purely independent simulations occupying the same space.

Understanding check must cover interaction ownership and momentum/occupancy exchange.

Codex model:

**Sol** by default.

Use **Astra in a new session** only if the cross-representation coupling architecture itself remains genuinely ambiguous/hard.

### 8. Temperature

Goal:

Add the minimum active temperature representation and thermal update.

Acceptance:

A heat source changes nearby material temperature in a predictable/debuggable way.

Understanding check:

* storage;
* initialization;
* diffusion/update;
* timestep/stability behavior;
* streaming behavior.

Codex model:

**Terra** if the representation and algorithm are already fully specified.

**Sol** if integrated GPU field design remains unresolved.

### 9. Ice <-> water transition

Goal:

Demonstrate a complete cross-form phase pair.

Acceptance:

Heating ice produces authoritative fluid particles and removes the corresponding cellular material.

Cooling water produces cellular ice and removes the corresponding fluid representation.

No duplicate authoritative material remains.

Understanding check must cover allocation/claiming and representation transfer.

Codex session:

`NEW`

Model: **GPT-5.6 Sol**

This is a likely hard integration task.

Escalate to **Astra** only for unresolved correctness/architecture problems.

### 10. Force/destruction slice

Goal:

Create the smallest real destructive interaction.

Suggested first behavior:

fixed/breakable material -> dynamic debris when an impulse threshold is exceeded.

Acceptance:

One observable event converts fixed material into moving material.

Understanding check:

* force source;
* accumulation;
* threshold;
* material transition;
* conservation shortcuts/limitations.

Codex model:

**Terra** if built entirely on already-settled force/state infrastructure.

Otherwise **Sol**.

### 11. Lighting

Goal:

Create a basic attractive GPU 2D lighting path.

Initial target:

* material albedo/opacity/emission;
* distance representation;
* ordinary Radiance Cascades;
* final composite.

Acceptance:

An emissive material illuminates/occludes the scene convincingly in realtime.

Understanding check must cover:

* lighting inputs;
* distance representation;
* cascade structure;
* cascade merging;
* final composition;
* resolution/performance choices.

Codex session:

`NEW`

Model: **GPT-5.6 Sol**

Use Astra only if genuinely needed for difficult RC implementation/debugging.

Do not start Holographic Radiance Cascades until the ordinary path works.

### 12. Rigid-body insertion point

Goal:

After the critical vertical slice works, validate the architectural path for cellular rigid bodies.

Preferred first concrete slice:

a rigid object containing body-local cellular material that can be rasterized into the world interaction representation.

Movement/dynamics may initially be minimal.

Acceptance:

The representation and rasterization path exist without requiring other physics systems to be redesigned.

This is a high-value external-reference gate.

Codex session:

`NEW`

Model:

**GPT-6 Astra** for initial architecture if major representation/coupling questions remain.

Once architecture is settled, stop that session and use a new **Sol** session for routine implementation.

### 13. Stabilization

Goal:

Stop expanding architecture.

Focus on:

* correctness bugs;
* visual/debug tools;
* performance;
* useful material presets;
* demo scene;
* game-jam usability.

Codex should use **Luna**, **Terra**, or **Sol** task-by-task rather than keeping one giant cleanup session.

Astra should normally not be used here unless a severe hard bug requires it.

## Codex Session Ledger

Use this section to track sessions that matter across chats.

Do not treat it as Git history.

Template:

### Session N — <objective>

* Status: planned / active / complete / abandoned
* Model:
* Started:
* Objective:
* Scope:
* Stop condition:
* Result:
* Important files changed:
* Follow-up session:
* Understanding check: pending / complete

No active implementation session yet.

## Understanding Ledger

Only the user/planning chat may mark these complete.

### Core world representation

Status: pending

Must be able to explain:

* tile versus chunk versus cell;
* active versus buffered area;
* logical world tile/cell -> physical ring slot;
* chunk persistence versus GPU simulation ownership.

### Material identity

Status: pending

Must be able to explain:

* material form bits;
* form-local index;
* material registry;
* graphics table lookup;
* why fluid IDs can exist even though fluid particles are not authoritative cellular objects.

### Actor/control path

Status: pending

Must be able to explain:

* input translation;
* possession;
* ECS components;
* actor position/velocity ownership;
* where physical collision will enter the flow.

Add subsystem-specific checkpoints here as implementation proceeds.

## External Reference Requests

No reference currently required.

When required, record:

* system/question;
* exact requested clip/text;
* what decision it should help resolve;
* resulting conclusion.

## Blockers

None recorded yet.

## Scope Cuts

If schedule pressure requires cuts, prefer:

* no Holographic RC;
* minimal rigid-body work;
* simple destruction only;
* no generalized fracture;
* no elaborate thermodynamics;
* no chemistry system;
* no persistent inactive fluid simulation;
* simple player controller;
* simple material appearance.

Preserve the core integrated cellular + granular + fluid-double-representation experiment if possible.

## Next Immediate Action

1. User reviews these planning files.
2. User creates/commits them manually.
3. In planning chat, inspect the active tile-ring mapping and design Task 1 precisely.
4. User completes the first world-representation understanding check.
5. Generate a tightly bounded Codex prompt.
6. Start a **new GPT-5.6 Sol Codex session** for the first actual GPU simulation slice.
7. Stop that session as soon as its narrow acceptance condition is satisfied and the handoff is complete.
