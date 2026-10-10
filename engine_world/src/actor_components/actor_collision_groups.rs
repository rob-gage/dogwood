// Copyright Rob Gage 2026

use bevy_ecs::component::Component;

/// Collision groups deciding which actors physically collide
#[derive(Component, Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct ActorCollisionGroupsComponent;
