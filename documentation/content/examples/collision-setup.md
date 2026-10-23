+++
title = "Actor collision"
description = "Configure actor bodies, collider shapes, masks, and contact sensors."
weight = 40
[extra]
sources = ["engine_world/src/scenes/collisions/collider.rs", "engine_world/src/scenes/collisions/collider_shape.rs", "engine_world/src/scenes/collisions/collision_body.rs", "engine_world/src/scenes/collisions/collision.rs", "engine_world/src/scenes/collisions/mod.rs", "engine_world/src/scenes/scene_error.rs", "engine_world/src/scenes/edits/edit_mode.rs", "engine_world/src/scenes/cellular_tiles/cellular_tiles.rs", "engine_world/src/actors/components/actor_collision.rs", "engine_world/src/actors/components/actor_contact.rs", "engine/src/lib.rs", "template/src/template_game.rs"]
+++

## Give an actor a body

Give an actor a physical presence by adding `ActorCollisionComponent` to an actor that has a position. Collider rules:

- **Units:** tiles; rotation is counterclockwise radians. Offsets are relative to the owner.
- **Shapes:** circle (`radius`), rectangle (`width`, `height`), and capsule (`width`, `height`). Rectangles are axis-aligned before rotation; capsules are vertical before rotation.
- **Valid values:** Dimensions must be finite and positive; capsule width must not exceed its height. Offsets and rotation must be finite. For cellular interactions, `mass` must be finite and positive when a kinematic or dynamic actor’s filter includes `COLLISION_GROUP_CELLULAR_TILE`. Fixed actors use an engine-defined mass.
- **Errors:** invalid values can produce `SceneError::InvalidCollider`.

`memberships` is the set of groups this actor belongs to; `filter` is the set it accepts. Two colliders interact only when each filter accepts the other’s membership. The reserved groups are listed below. In this example, the body belongs to the actor and edit groups. Its filter accepts every group except settled dynamic cells; the engine removes that group from actor filters.

```rust
use engine::{
    actors::{components::ActorCollisionComponent, Actor},
    scenes::collisions::{
        Collider, ColliderShape, CollisionBody,
        COLLISION_GROUP_ACTOR, COLLISION_GROUP_CELLULAR_DYNAMIC, COLLISION_GROUP_EDIT,
    },
    scenes::Scene,
};

fn attach_body(scene: &mut Scene, live_actor: Actor) -> Result<(), Box<dyn std::error::Error>> {
    let body = ActorCollisionComponent {
        body: CollisionBody::Kinematic,
        collider: Collider {
            shape: ColliderShape::Circle { radius: 0.5 },
            offset_x: 0.0,
            offset_y: 0.0,
            rotation: 0.0,
        },
        memberships: COLLISION_GROUP_ACTOR | COLLISION_GROUP_EDIT,
        // every group except settled dynamic cells
        filter: u32::MAX & !COLLISION_GROUP_CELLULAR_DYNAMIC,
        mass: 40.0,
    };
    scene.actor_registry_mutable().insert(live_actor, body)?;
    Ok(())
}
```

### Body kinds

| Body | Moves when |
| --- | --- |
| `Fixed` | Never under collision simulation; changing the actor's position repositions it |
| `Kinematic` | Its owner's position is set; pawn movement can also move kinematic actors configured as pawns |
| `Dynamic` | Simulated forces and collisions act on it |

`mass` is in dynamic cellular material mass units. It affects how the actor pushes dynamic cells and how much they slow it. The template uses a kinematic body with a circle collider and the same membership and filter pattern.

## Choose collision groups and contacts

Use the exported constants for reserved groups, and assign custom groups to other bits.

| Constant | Meaning |
| --- | --- |
| `COLLISION_GROUP_CELLULAR_DYNAMIC` | Settled dynamic cells |
| `COLLISION_GROUP_ACTOR` | Actor colliders |
| `COLLISION_GROUP_CELLULAR_TILE` | Solid cells of the active cellular tiles |
| `COLLISION_GROUP_RIGID_BODY` | Cellular rigid bodies |
| `COLLISION_GROUP_EDIT` | Actor collision colliders whose overlapping cells `EditMode::Insert` skips |

The engine ignores `COLLISION_GROUP_CELLULAR_DYNAMIC` in actor and contact collider filters. Actor-cell kinematics instead checks whether an actor filter includes `COLLISION_GROUP_CELLULAR_TILE`.

### Contact sensors

`ActorContactComponent` configures a contact sensor with its own collider, memberships, and filter. The engine inserts it as an internal sensor but exposes no gameplay query or event for its overlaps. Contact data for dynamic cells and cellular rigid bodies is likewise internal, not a contact-query API. See [actor components](@/external-api/actor-components.md) and [collisions and edits](@/external-api/collisions-and-edits.md) for the reference.

{{ <rustdoc path="engine/scenes/collisions/struct.Collider.html" text="Collider" /> }} · {{ <rustdoc path="engine/scenes/collisions/enum.ColliderShape.html" text="ColliderShape" /> }} · {{ <rustdoc path="engine/scenes/collisions/enum.CollisionBody.html" text="CollisionBody" /> }}
