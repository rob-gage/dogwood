// Copyright Rob Gage 2026

use bevy_ecs::component::Component;

/// Linear velocity of an actor
#[derive(Component, Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct ActorKinematicsVelocityComponent;
