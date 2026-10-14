// Copyright Rob Gage 2026

use bevy_ecs::component::Component;

/// Angular velocity of an actor
#[derive(Component, Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct ActorKinematicsAngularVelocityComponent;
