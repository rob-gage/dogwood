## Verdict

NOT READY

The GPU ownership/reaction/readback architecture is sound. The proposed contact solver is **not yet sufficient to replace rigid↔sand terrain while preserving support**. A support/contact correction must be settled before handing this off as one Sol implementation session.

Read-only inspection completed; no files changed, tests run, formatting, or Git operations performed.

Documentation mismatch: `PLAN.md` describes several implemented systems as future work; `SPRINT.md` still lists editing, sand, and fluids as unimplemented. This report follows the working tree.

## Critical corrections

1. **Finite-mass impact response does not establish persistent pile support.**  
   The current solver resolves contacts once from immutable velocities. It does not iterate support impulses through the pile back into the rigid body. `CellularDynamic` subsequently removes blocked gravity-directed velocity without returning that discarded momentum.

   Concrete scale check: a 32×32 rigid block has mass 1,024 under current collider density. Its gravity impulse is 307.2 per tick at gravity 18. Thirty-two stationary sand contacts against a downward velocity of 0.3 produce only about 10 units of proposed upward reaction, even before angular effective-mass reductions. Continued sinking is a credible failure, not something waking fixes.

   **Correction:** settle a concrete support-aware contact response, including overlap handling, and validate it on a heavy block over confined sand. Reciprocal impact impulses alone cannot be the acceptance criterion.

2. **Removing rigid collision regions does not eliminate all Rapier sand contacts.**  
   Pawn-local sand colliders remain ordinary parentless colliders with default interaction groups. Nearby rigid bodies would still collide with them.

   **Correction:** exclude rigid↔dynamic-terrain pairs through collider collision groups while retaining pawn scene queries, rigid↔static, and rigid↔rigid interactions.

3. **Use consistent units and distinguish grains from overlays.**  
   Rapier positions and cellular velocities use tile units; integer world-cell coordinates use cell units. Also, current contact helpers substitute proxy velocity and immovable mass whenever occupancy is nonzero—even if canonical sand occupies that same location.

   **Correction:** convert contact positions to tile units and read actual grain mass/snapshot velocity for granular sources. Do not accidentally process proxy-covered grains as million-mass particles or produce self-body reactions.

4. **Index collection and mutation must be transactional.**  
   `Scene::tick` uses `filter_map`; removal calls `swap_remove` before a fallible state lookup.

   **Correction:** preserve one transform record per body index and validate handles before mutating topology. Every topology mutation must invalidate pending reactions and rebuild indexed GPU state.

5. **Contact count must include resting contact, and accumulation needs an overflow policy.**  
   Counting only positive impulses misses sleeping support. Indefinitely retaining reactions in finite integers cannot guarantee preservation.

   **Correction:** count geometric contacts before the approach-speed early exit; track support/contact history on the CPU. Establish a bounded operational accumulation envelope and explicit error handling rather than silently wrapping or dropping reactions.

## Rapier API facts

Verified against locally installed Rapier **0.35.3**:

| Purpose | API/property |
|---|---|
| Scalar inverse mass | `body.mass_properties().local_mprops.inv_mass` |
| Effective inverse translation mass | `body.mass_properties().effective_inv_mass` — a vector accounting for locked axes |
| Effective inverse angular inertia | `body.mass_properties().effective_world_inv_inertia` — scalar in 2D |
| World COM | `body.center_of_mass()` |
| Linear impulse | `body.apply_impulse(Vector::new(x, y), true)` |
| Angular impulse | `body.apply_torque_impulse(angular_impulse, true)` |
| Explicit wake | `body.wake_up(true)` |
| Sleep inspection | `body.is_sleeping()` |

Current rigid bodies are dynamic with unlocked axes, so scalar inverse mass fits the proposed record. Assert that assumption; scalar mass cannot represent independently locked translation axes.

The inverse angular inertia requires **no orientation conversion in 2D**. Despite a stale “square-root” field comment, the implementation uses this value directly as inverse inertia; **do not square it**. Local Parry’s 2D `world_inv_inertia` returns `inv_principal_inertia`.

The COM currently uploaded by `rigid_cellular_body_state` is the correct world COM, in tile units.

## Contact math check

The signs match [current pressure conventions](/home/rmg/Documents/dogwood-fresh/engine_physics/src/simulation/cellular_pressure.wgsl):

- `normal`: granular cell toward neighboring rigid cell.
- `relative_velocity = v_cell - v_body`.
- Positive dot product means approach.
- `J_cell = -normal * j`; `J_rigid = +normal * j`.
- Angular impulse is `r.x * J_rigid.y - r.y * J_rigid.x`, positive counterclockwise.

Use:

`contact_point = (vec2(world_cell) + 0.5 + normal * 0.5) / 8`

Then calculate `r` against the uploaded COM. The proposed effective-mass equation is correct for an isolated contact between an unlocked rigid body and a freely responding grain.

The rasterized velocity is evaluated at the **rigid cell center**, not the shared face. Its normal component is unchanged by moving half a cell along the normal, so normal response remains correct. For friction, evaluate point velocity at the face using the existing transform/motion records.

Preserve the existing friction correction:

`Δv_t = -relative_tangent_velocity * min(1, min(friction_a, friction_b) * dt * 8)`

Accumulate `-mass_cell * Δv_t` and its torque into the body. Use the actual rigid material identifier with existing material tables. Current proxy friction is zero, so enabling material friction is an intentional behavior change.

These equations conserve pairwise linear impulse, subject to quantization. They do **not** guarantee stability for many simultaneous contacts sharing unchanged rigid velocity or for delayed application.

## Pressure double-counting check

**Yes, retaining the existing rigid-contact gather creates an additional effective response.**

`resolve_cellular_contacts` currently:

1. Changes granular velocity.
2. Independently calls `gather_cellular_contact_pressure`.
3. Injects another **2%** impact-pressure contribution at the destination.
4. Propagates that pressure through six passes.
5. Applies retained pressure as additional granular velocity or static damage.

That extra contribution is not subtracted from the original contact impulse. Adding the full opposite rigid impulse while retaining this source would preserve an additional, unaccounted pressure response.

Minimal change:

- Accumulate rigid normal/friction reactions in the rigid-neighbor branch of contact resolution.
- Exclude that same rigid/granular pair from pressure gathering, including proxy-covered source cases.
- Preserve existing dynamic↔dynamic, dynamic↔static, and pawn branches.
- Do not also accumulate the same impulse from retained pressure.

Rigid proxy cells can retain their existing transmission value of `1.0` for unrelated pressure. However, this makes them pressure conduits, **not a reciprocal rigid support solver**. Retained pressure currently has no authoritative rigid-body destination.

## GPU atomic/readback check

`atomic<i32>` and `atomic<u32>` storage atomics are supported without float-atomic features. The proposed record occupies **16 bytes**. [WGSL specification](https://www.w3.org/TR/WGSL/#atomic-types)

A reasonable initial precision/range tradeoff is:

- Linear scale **256**: resolution 0.00390625; range approximately **±8.39 million** impulse units.
- Angular scale **64**: resolution 0.015625; range approximately **±33.55 million** angular-impulse units.

These are starting recommendations, **not proven safe worst-case bounds**. Sand normally exits movement capped at 30 tiles/s, but rigid point velocity, pressure-driven velocity, lever arms, and readback delay lack sufficient global bounds. Require:

`scale × sum(abs(component contributions)) < 2³¹`

over the entire uncopied interval. Quantize symmetrically; validate finite inputs. Exact opposition is otherwise only approximate to fixed-point precision.

Per-contact atomics are a sensible first implementation: current detached bodies contain at most 1,024 cells, and many bodies distribute contention. No inspected existing reduction approach is clearly superior.

**One staging slot is logically safe:**

1. Finish contact writes.
2. Copy `body_count × 16` bytes.
3. Clear that accumulator range in the same encoder, **after the copy**.
4. Submit.
5. Request staging `map_async`.
6. Decode, release the mapped view, unmap, and consume exactly once.

Later submissions on the same queue can write fresh reactions while the copied staging data awaits mapping. Never clear the accumulator from the completion callback. [WebGPU synchronization model](https://www.w3.org/TR/webgpu/#programming-model-synchronization)

Busy-slot skips preserve uncopied impulses, provided no overflow, topology reset, or device/readback failure occurs. This preserves accounting—not necessarily stable delayed physics.

Three slots reuse the existing collision-readback pattern and reduce missed copy opportunities. They do not guarantee bounded latency. Unlike occupancy snapshots, **every compatible reaction batch must be consumed; “newest snapshot wins” is incorrect**.

## Body-index/topology hazards

Actual sites:

- [Scene tick](/home/rmg/Documents/dogwood-fresh/engine_physics/src/scenes/scene.rs:727): `filter_map` collapses missing handles while rigid-cell upload still enumerates the original vector.
- `detach_unanchored_static_components`: appends bodies and increments revision. Existing indices remain stable, but count/capacity changes.
- [Cell removal/splitting](/home/rmg/Documents/dogwood-fresh/engine_physics/src/scenes/scene.rs:923): `swap_remove` moves the last body; connected components are appended with new Rapier handles.
- That removal path currently returns early on missing state **after reordering**, without incrementing revision. It is presently marked dead code, but is a real invalidation hazard.

Writing `body_index + 1` in `resolve_rigid_cell_proxy` is correct: claim resolution already selects one source. Clear owners each raster; pawn cells retain zero.

Capture revision and count per readback; verify both before application. On topology change, clear the old accumulator before new-topology contacts and reject stale mapped batches. This intentionally discards old reactions; it is safe indexing, **not momentum preservation across splitting**.

## Sleeping/support behavior

Sleeping bodies are currently rasterized every tick: state collection does not filter sleeping bodies.

Minimum wake bookkeeping:

- Count resting geometric granular contacts even when `approach_speed == 0`.
- Retain a per-body “had granular contact” state across readbacks.
- Strong-wake contacted bodies before active Rapier steps while that state remains set.
- On contact→no-contact transition, wake once before clearing the state.
- Apply nonzero reactions with wake enabled.
- Preserve normal sleeping for bodies without granular contact.

An accumulated count means “contact occurred during this interval,” not necessarily “contact exists now.” A later zero-contact batch clears it conservatively.

`wake_up(true)` resets Rapier’s sleep timer; zero-valued impulse calls do not wake bodies. Contact-only waking cannot supply the missing support force identified above.

## Exact implementation scope for Sol

**Conditional plan: resolve step 1 before authorizing the complete bridge replacement.**

1. Settle and exercise a confined-sand support case: heavy block, off-center load, support erasure, and proxy overlap. Specify how pile support reaches the rigid reaction.
2. In `scene.rs` and `scene_physics_world.rs`, fix index preservation and mutation ordering; expose authoritative mass properties through crate-private state plumbing.
3. In `cellular_physics_body_proxy.rs/.wgsl`, add owner storage, clear/write ownership, and fill transform record spare components.
4. In `cellular_pressure.rs/.wgsl`, add rigid bindings and reciprocal contact/friction handling; eliminate duplicate gather sources and distinguish grains from overlays.
5. Add private reaction accumulation/readback ownership and dedicated slot/status files following existing module conventions. Check storage-buffer limits: pressure already uses 15 storage bindings, or 16 in compaction.
6. Add compatible-batch collection, once-only application, topology resets, and contact-history waking. Collect after polling; apply at an active fixed-tick boundary before Rapier steps.
7. Preserve current order: terrain update → Rapier → actors → rigid raster → pressure/contact → reaction copy → dynamic movement → fluids → gases → collision extraction. Moving pressure after movement would observe velocity already clipped by blocked movement; it does not fix support.
8. In `ScenePhysicsWorld`, filter rigid bodies out of pawn-local sand collider interactions, then remove the rigid-region extension and `rigid_cellular_body_collision_regions`.
9. Validate momentum/torque signs, resting support/removal, overlap, delayed readbacks, topology changes, pawn collision, static/rigid collision, and the reported many-body sand performance case.

Only the rigid-region helper and its Scene call become obsolete. Dynamic terrain buffers/state, region merging, collider replacement, collision extraction, and CCD-cache invalidation remain needed.

## Do not do

- Do not equate reciprocal impact impulses with a solved support constraint.
- Do not retain hidden rigid↔pawn-local-sand Rapier collision.
- Do not use proxy mass/velocity as canonical grain state.
- Do not multiply impulses by `dt` again.
- Do not discard older compatible reaction batches.
- Do not clear reactions every tick or on mapping completion.
- Do not add generalized reduction/contact frameworks or globally disable sleeping.
- Do not change pawn behavior, materials, streaming, or documentation opportunistically.

## Confidence

**75%** that the architecture, with the identified corrections, can replace the bridge successfully.

The unresolved fact is whether a narrowly scoped cellular support response can keep heavy rigid bodies supported without persistent sinking, overlap artifacts, or unstable delayed multi-contact impulses. The current velocity-only formula and pressure propagation do not establish that. The confined-sand support case should resolve this before the full Sol implementation session.
