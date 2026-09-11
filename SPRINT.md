# Dogwood Vertical-Slice Sprint

Sprint initialized: 2026-09-10

Target: working game-jam-ready engine vertical slice within approximately seven days.

This file is deliberately volatile. Update it as implementation reality changes. `PLAN.md` remains the durable architecture document.

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
* WGPU `Accelerator` and GPU storage-buffer allocation.
* `Scene`, 60 Hz fixed simulation scheduling, chunk persistence/generation, and asynchronous chunk streaming.
* 8x8-cell tiles and 64x64-tile chunks.
* Streamed GPU-resident active/buffered cellular world using the two-dimensional tile ring.
* Cellular material-ID uploads/downloads.
* Static cellular world generation and rendering from the GPU material-ID buffer.
* Persistent per-cell appearance.
* Form-specific GPU material appearance tables.
* `CellularStatic`, `CellularDynamic`, and `Fluid` material forms.
* Form-tagged `MaterialIdentifier` and the loaded `MaterialRegistry`.
* Scene gravity.
* `bevy_ecs` actor registry, possessable walking pawn, actor position/velocity, and input/control-state routing.
* Rapier-backed cellular collision and walking movement.
* Editor Free Fly / Return and current editor viewport integration.

### Still required

* CPU-controlled SceneEdit placement/erase boundary.
* Independent editor pause, mouse/view plumbing, material palette, and brush painting.
* GPU-simulated `CellularDynamic` Sand.
* Authoritative fluid particle storage, PBF simulation, and derived fluid-cellular representation.
* Granular/fluid coupling.
* Temperature, ice/water phase transitions, force/impulse infrastructure, destruction, rigid bodies, lighting, thermal material rendering, and significant automated simulation validation.

## Immediate Work Tracks

### Track A — Scene editing/editor

#### A1 — SceneEdit boundary

Next task:

* Add the narrow CPU-controlled, concrete `SceneEdit`-style placement/erase boundary owned by `Scene`.
* Use world-cell coordinates where needed.
* Initialize placement defaults from the owning material, including `MaterialIdentifier` and persistent `CellularAppearance`.
* Mutate resident GPU state immediately.
* Synchronize and dirty resident CPU chunk persistence.
* Invalidate or refresh collision data.
* Do not add editor UI yet.

#### A2 — Editor controls

Add editor play/pause independent from `Game::is_paused()`, plus mouse and view plumbing. Paused editor mode must continue rendering, UI, Free Fly, streaming, and explicit edits while suppressing ordinary gameplay/cellular/fluid simulation.

#### A3 — Palette and brush

Use the loaded `Scene`'s real `MaterialRegistry`. Add material palette, Eraser, Square/Circle brush modes, wheel sizing, and exact preview/painting. Preview and painting must share the same discrete world-cell rasterization.

### Track B — granular Sand

Still required:

* GPU-simulated `CellularDynamic` Sand;
* arbitrary Scene gravity;
* safe conflict handling;
* falling and diagonal settling;
* correct tile/ring behavior;
* movement of all matter-attached state together;
* no editor-specific Sand logic.

### Track C — fluid/PBF

Still required:

* authoritative fluid particle storage;
* support-radius-based spatial indexing independent of tile size;
* the PBF solver;
* a derived fluid-cellular representation;
* granular/fluid coupling.

## Coordination Constraint

Editor SceneEdit work and Sand work may both touch `Scene` and active-cell ownership. Do not run autonomous concurrent Codex sessions editing the same Scene files.

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

GPU state is authoritative for active cellular simulation. Chunks remain persistence/streaming storage.

### Materials

Continue using `MaterialIdentifier` and current material forms. Simulation properties derive from the existing registry.

### Fluids

Initial preferred solver: PBF. Fluid particle state is authoritative. Cellular fluid data is derived. Fluid spatial buckets are independent of the 8x8 tile size.

### Rigid bodies

Long-term preferred representation: body-local cellular data plus transform/velocity as authoritative; world-space cellular rasterization as derived interaction data. Full rigid-body dynamics are not on the initial critical path.

### Lighting

Implement ordinary Radiance Cascades before experimenting with Holographic Radiance Cascades unless sprint conditions change.

### API philosophy

Prefer private subsystem implementation. Generalize only after multiple real consumers prove the abstraction.

## Later Work

After Tracks A–C, preserve the planned work for:

* temperature representation and persistence;
* ice/water phase transitions without duplicate authoritative material;
* force/impulse interactions and destruction/material detachment;
* lighting reintegration and thermal material rendering;
* the rigid-body insertion path;
* stabilization, demo scenes, visual/debug tools, performance, and game-jam usability.

Resolve granular conflict handling, exact fluid-cellular raster format, cross-form transition allocation, and rigid interaction overlays immediately before their implementations rather than guessing them now.

## Codex Session Ledger

Use this section to track sessions that matter across chats. Do not treat it as Git history.

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

Must be able to explain tile versus chunk versus cell; active versus buffered area; logical world tile/cell to physical ring slot; and chunk persistence versus GPU simulation ownership.

### Material identity

Status: pending

Must be able to explain material form bits, form-local index, material registry, graphics table lookup, and why fluid IDs can exist even though fluid particles are not authoritative cellular objects.

### Actor/control path

Status: pending

Must be able to explain input translation, possession, ECS components, actor position/velocity ownership, and where physical collision enters the flow.

Add subsystem-specific checkpoints as implementation proceeds.

## External Reference Requests

No reference currently required.

When required, record the system/question, exact requested clip/text, decision it should help resolve, and resulting conclusion.

## Blockers

None recorded yet.

## Scope Cuts

If schedule pressure requires cuts, prefer no Holographic RC, minimal rigid-body work, simple destruction only, no generalized fracture, no elaborate thermodynamics, no chemistry system, no persistent inactive fluid simulation, and a simple player controller.

Preserve the core integrated cellular + granular + fluid-double-representation experiment if possible.

## Next Immediate Action

1. User reviews these planning files.
2. User creates/commits them manually.
3. In planning chat, design A1 precisely against the active tile-ring mapping and Scene ownership.
4. User completes the core world-representation understanding check.
5. Generate a tightly bounded Codex prompt for A1.
