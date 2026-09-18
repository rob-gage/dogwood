// Copyright Rob Gage 2026

use rapier2d::prelude::Group;
use rapier2d::prelude::InteractionGroups;

use super::scene_physics_world::ScenePhysicsWorld;

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
            .with_filter(Group::GROUP_1 | Group::GROUP_5)
    }

    pub(crate) fn terrain_solver_groups() -> InteractionGroups {
        InteractionGroups::all()
            .with_memberships(Group::GROUP_3)
            .with_filter(Group::GROUP_1 | Group::GROUP_5)
    }

    pub(crate) fn dynamic_collision_groups() -> InteractionGroups {
        InteractionGroups::all()
            .with_memberships(Group::GROUP_4)
            .with_filter(Group::GROUP_1 | Group::GROUP_5)
    }

    pub(crate) fn dynamic_solver_groups() -> InteractionGroups {
        InteractionGroups::all()
            .with_memberships(Group::GROUP_4)
            .with_filter(Group::GROUP_1 | Group::GROUP_5)
    }

    pub(crate) fn pawn_collision_groups() -> InteractionGroups {
        InteractionGroups::all()
            .with_memberships(Group::GROUP_2)
            .with_filter(Group::GROUP_1 | Group::GROUP_5)
    }

    pub(crate) fn pawn_solver_groups() -> InteractionGroups {
        InteractionGroups::all()
            .with_memberships(Group::GROUP_2)
            .with_filter(Group::GROUP_1 | Group::GROUP_5)
    }

    pub(crate) fn physical_collision_groups() -> InteractionGroups {
        InteractionGroups::all()
            .with_memberships(Group::GROUP_5)
            .with_filter(
                Group::GROUP_1 | Group::GROUP_2 | Group::GROUP_3 | Group::GROUP_4 | Group::GROUP_5,
            )
    }

    pub(crate) fn physical_solver_groups() -> InteractionGroups {
        Self::physical_collision_groups()
    }

    pub(crate) fn pawn_query_groups() -> InteractionGroups {
        InteractionGroups::all()
            .with_memberships(Group::GROUP_1 | Group::GROUP_3 | Group::GROUP_4)
            .with_filter(Group::GROUP_1 | Group::GROUP_3 | Group::GROUP_4 | Group::GROUP_5)
    }
}
