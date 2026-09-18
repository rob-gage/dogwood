// Copyright Rob Gage 2026

use super::scene_physics_world::ScenePhysicsWorld;
use rapier2d::prelude::{Group, InteractionGroups};

impl ScenePhysicsWorld {
    pub(crate) fn rigid_solver_groups() -> InteractionGroups {
        InteractionGroups::all()
            .with_memberships(Group::GROUP_1)
            .with_filter(Group::ALL)
    }

    pub(crate) fn rigid_collision_groups() -> InteractionGroups {
        InteractionGroups::all()
            .with_memberships(Group::GROUP_1)
            .with_filter(Group::ALL)
    }

    pub(crate) fn terrain_collision_groups() -> InteractionGroups {
        InteractionGroups::all()
            .with_memberships(Group::GROUP_3)
            .with_filter(Group::GROUP_1)
    }

    pub(crate) fn terrain_solver_groups() -> InteractionGroups {
        InteractionGroups::all()
            .with_memberships(Group::GROUP_3)
            .with_filter(Group::GROUP_1)
    }

    pub(crate) fn dynamic_collision_groups() -> InteractionGroups {
        InteractionGroups::all()
            .with_memberships(Group::GROUP_4)
            .with_filter(Group::GROUP_1)
    }

    pub(crate) fn dynamic_solver_groups() -> InteractionGroups {
        InteractionGroups::all()
            .with_memberships(Group::GROUP_4)
            .with_filter(Group::GROUP_1)
    }

    pub(crate) fn pawn_collision_groups() -> InteractionGroups {
        InteractionGroups::all()
            .with_memberships(Group::GROUP_2)
            .with_filter(Group::GROUP_1)
    }

    pub(crate) fn pawn_solver_groups() -> InteractionGroups {
        InteractionGroups::all()
            .with_memberships(Group::GROUP_2)
            .with_filter(Group::GROUP_1)
    }

    pub(crate) fn pawn_query_groups() -> InteractionGroups {
        InteractionGroups::all()
            .with_memberships(Group::GROUP_1 | Group::GROUP_3 | Group::GROUP_4)
            .with_filter(Group::GROUP_1 | Group::GROUP_3 | Group::GROUP_4)
    }
}
