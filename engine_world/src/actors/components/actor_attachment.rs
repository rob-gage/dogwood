// Copyright Rob Gage 2026

use bevy_ecs::component::Component;

/// Attaches an actor to a parent actor
#[derive(Component, Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct ActorAttachmentComponent;
