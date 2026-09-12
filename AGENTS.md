# AGENTS

Dogwood is a Rust/WGPU game engine for highly interactive 2D simulated worlds.

Read `PLAN.md` and `SPRINT.md` before making substantive changes.

`PLAN.md` contains durable architecture and project direction.

`SPRINT.md` contains the current implementation state, priorities, unresolved decisions, blockers, acceptance criteria, and understanding checkpoints.

If documentation conflicts with the actual code, inspect the code and report the discrepancy. Do not silently rewrite architecture to make the documents appear correct.

## Primary Objective

During the current sprint, optimize for a working, understandable, integrated vertical slice.

Correct integration is more valuable than speculative generality.

Every implementation task should leave Dogwood closer to an executable game-jam-ready demonstration.

## Code Discipline

* Do not modify unrelated files or unrelated portions of files.
* Preserve existing code style and formatting conventions.
* Normally place exactly one primary named `struct`, `enum`, or `trait` in each source file.
* Every new primary type MUST get its own appropriately named source file and MUST follow the existing module declaration and re-export structure.
* Do not place unrelated structs, enums, or traits in an existing file merely because that file is already being edited.
* Avoid module-level free functions when behavior naturally belongs to an owning type.
* Private helper behavior normally MUST be implemented as methods on the appropriate type.
* Before writing code, inspect neighboring source files and imitate their organization, naming, visibility, imports, comments, line wrapping, indentation, brace placement, and vertical whitespace.
* Preserve the existing blank-line style inside `impl` blocks, including blank lines after the opening brace and before the closing brace wherever surrounding Dogwood files use them.
* Do not place a final method directly against an `impl` closing brace when the surrounding project convention leaves a blank line there.
* NEVER RUN `rustfmt`.
* Reuse existing infrastructure before creating new infrastructure.
* Preserve working module boundaries unless they concretely block the current task.
* Prefer private/internal implementation over new public API.
* Do not perform opportunistic cleanup during feature tasks.
* Do not opportunistically reorganize imports, move methods or types between files, rename modules, or perform unrelated cleanup.
* Do not redesign functioning infrastructure merely because an isolated subsystem could be cleaner another way.
* Do not add dependencies without a concrete need.
* Keep authoritative state and derived state explicitly distinguished.
* Avoid unnecessary CPU/GPU synchronization or readback.
* Make concurrency assumptions explicit.
* Before handoff, reread every changed source file specifically for structural and style consistency.
* Source-organization and style violations are implementation defects, not optional cleanup.

## Anti-Overengineering Rule

Do not introduce a public abstraction, trait, wrapper, manager, registry, generalized framework, new crate, or extensibility layer merely because future systems might use it.

Add the smallest implementation required by the current concrete task.

Generalize only after multiple implemented consumers demonstrate the common abstraction.

Examples of abstractions that must not be created speculatively include:

* generic compute graphs;
* generic physics-field systems;
* generic particle frameworks;
* generic constraint solvers;
* generic collision-object traits;
* generic phase-transition graphs;
* generic physics-rule engines;
* generic render graphs.

A subsystem may directly and privately own its pipelines, bind groups, buffers, scratch data, and shaders.

Duplication across the first implementation is preferable to locking the engine into the wrong abstraction. Extract common infrastructure after the real pattern is understood.

## Existing Architecture to Preserve

Unless a task demonstrates a concrete blocker:

* `Scene` remains the integration point for active-world simulation and streaming.
* Preserve the existing chunk and 2D tile-ring system.
* Tiles remain 8x8 cells.
* Chunks remain 64x64 tiles.
* Simulation partitioning must not automatically use tile/chunk boundaries when another algorithm requires a different spatial scale.
* Active cellular state is GPU-oriented.
* CPU chunks are primarily persistence, streaming, and inactive-world storage.
* `MaterialIdentifier` remains the common material identity mechanism.
* Static cellular, dynamic cellular, and fluid forms remain distinct.
* GPU physics/graphics material data should derive from the same registered materials.
* Actors remain in the existing `bevy_ecs`-backed `ActorRegistry`.
* Existing possession/control-state infrastructure should be reused.
* `ScenePosition` and `SceneVelocity` remain continuous actor/world quantities.
* Fluid particles are authoritative over their derived cellular representation.
* Future rigid bodies should be compatible with an authoritative body-local cellular representation and a derived world-space rasterization.
* Transient GPU state does not become persistent chunk data without a concrete persistence requirement.

## Before Implementing a Task

Inspect every relevant existing type and code path first.

Before editing, establish:

1. What existing infrastructure is being reused?
2. What representation is authoritative?
3. What minimum new state is required?
4. What minimum new API is required?
5. Can that API remain private?
6. Which files are actually in scope?
7. What is explicitly out of scope?
8. What is the observable acceptance condition?
9. What correctness/concurrency hazards exist?
10. Which `PLAN.md` or `SPRINT.md` assumptions does the task depend upon?

Do not implement a materially different design from the supplied task without surfacing the issue first.

## During Implementation

Keep changes narrow.

When adding GPU simulation:

* keep buffer ownership obvious;
* keep pass ordering obvious;
* use descriptive shader/pass names;
* document non-obvious indexing/concurrency invariants;
* avoid hidden CPU readbacks;
* avoid making streaming dimensions accidentally determine simulation-algorithm dimensions;
* ensure tile-ring wrapping and world-coordinate mapping are respected where relevant.

When using double representations:

* state which one is authoritative;
* state when/how the derived representation is generated;
* do not simulate both independently;
* do not allow duplication during cross-representation transitions.

When adding material properties:

* add only properties required by implemented behavior;
* derive GPU tables from the existing material registry;
* do not create an unrelated material database.

## Validation

Compilation alone is not sufficient for simulation work.

Each task requires the narrowest practical observable validation.

Examples:

* a sand column falls and settles;
* debug colors show the correct PBF bucket per particle;
* fluid density error converges;
* fluid cellular coverage follows particles;
* a pawn cannot pass through stone;
* ice visibly creates water particles after heating;
* freezing removes fluid particles before creating ice;
* a destruction threshold turns fixed material into debris;
* an emissive material visibly affects the lighting result.

Prefer small deterministic/debug scenes over broad test infrastructure during the sprint.

## Handoff Required at End of Every Task

Provide:

### Changed files

State every file changed and why.

### Existing infrastructure reused

Name the existing Dogwood types, buffers, APIs, or ownership paths used by the implementation.

### New infrastructure

List every new:

* type;
* significant field;
* GPU buffer;
* pipeline;
* bind group;
* shader/pass;
* public method/API.

Explicitly state if no new public API was introduced.

### Data ownership

Explain authoritative versus derived state and CPU versus GPU ownership.

### Execution order

Describe the exact important data flow/pass order.

### Validation

State exactly what was run or observed and the result.

### Limitations

State known shortcuts, missing cases, performance concerns, and intentionally deferred work.

### Documentation mismatch

State whether any assumption in `PLAN.md` or `SPRINT.md` proved incorrect.

Do not silently change durable architecture to hide a mismatch.

### Understanding check candidates

Provide 2-5 questions testing the important concepts introduced by the change.

Do not mark those questions as completed.

## Human Understanding Requirement

The user must understand all substantial generated code.

Understanding verification happens outside Codex, generally in planning/review chat.

A subsystem is not considered understood merely because it works.

Good understanding questions test:

* authoritative representation;
* buffer layout;
* coordinate/index mapping;
* algorithm/pass order;
* synchronization;
* race avoidance;
* integration boundaries;
* important failure cases.

Avoid trivia.

## PLAN.md and SPRINT.md Editing

Do not modify `PLAN.md` unless the task explicitly asks for a durable architectural-plan change.

Do not modify `AGENTS.md` unless explicitly asked.

Modify `SPRINT.md` only if the task explicitly includes sprint-status maintenance.

Never mark a human understanding checkpoint complete yourself.

## Git Policy

The user has sole control over Git operations and repository history.

Do not:

* stage changes;
* commit;
* amend;
* create/delete/switch branches;
* merge;
* rebase;
* reset;
* push;
* pull;
* tag;
* modify Git configuration;
* otherwise manipulate repository Git state.

Do not invoke Git commands unless the user explicitly requests a read-only Git inspection.

Leave completed changes in the working tree.

The user will inspect the changes and decide what to commit.

This separation is intentional and is part of the user's review/understanding process.

## Codex Session Policy

One Codex session should represent one coherent implementation objective or a very tightly coupled set of tasks.

Use one model for the entire session.

Never switch models in an active session.

### Continue the current session when

* fixing a defect introduced by the current objective;
* getting the current objective to compile;
* completing its acceptance test;
* making a small directly necessary follow-up;
* answering questions needed to finish the same implementation.

### Stop the current session when

* its objective works;
* its acceptance condition has been exercised;
* the required handoff is complete;
* remaining work is a different independently reviewable objective;
* remaining work should use a different model tier.

### Start a new session when

* starting another subsystem;
* starting an independently reviewable implementation slice;
* moving from hard design/debugging to cheap mechanical work;
* escalating to a stronger model;
* the prior session contains enough completed unrelated context to become a liability.

If a cheaper-model session discovers a problem requiring substantially stronger reasoning, stop it and request a new session on a stronger model. Do not switch the existing session.

## Codex Model Selection

The planning chat should name the exact model before every new Codex session.

Current guidance:

### GPT-5.6 Luna

Use for:

* tiny mechanical changes;
* documentation/status editing;
* straightforward repetitive edits;
* trivial cleanup;
* very simple tests with an already explicit implementation.

Do not use Luna to decide architecture or nontrivial GPU/concurrency behavior.

### GPT-5.6 Terra

Use for:

* routine contained implementation;
* already-designed Rust plumbing;
* straightforward WGPU boilerplate;
* simple integration work;
* uncomplicated demo/test code.

### GPT-5.6 Sol

Use by default for substantial implementation involving:

* WGSL compute shaders;
* GPU algorithms;
* concurrent cellular updates;
* particle simulation;
* synchronization;
* changes crossing multiple Dogwood systems;
* nontrivial debugging.

### GPT-6 Astra

Reserve for:

* difficult architectural decisions;
* novel cross-representation coupling;
* hard GPU race/correctness problems;
* difficult performance architecture;
* hard debugging where Sol has not been sufficient.

Once Astra has resolved the hard part, use a new cheaper-model session for routine implementation if appropriate.

If these model names change or a model is unavailable, use the nearest currently available capability tier.

## External Reference Requests

The user can supply screenshots, videos, and developer comments from two other similar in-development game engines.

Ask for a reference only when it could materially resolve a concrete question.

Useful reference topics include:

* cellular dynamic update/conflict algorithms;
* fluid-cellular coupling;
* fluid/cellular dynamic interaction;
* rigid cellular body representation;
* destruction and cell detachment;
* thermal transitions;
* lighting.

Ask narrowly and explain what evidence is needed.

Example:

“Please send the clearest fluid-versus-sand clip and any nearby technical comment describing whether their fluid is rasterized into the cellular grid.”

Do not request an indiscriminate dump unless broad comparison is specifically necessary.
