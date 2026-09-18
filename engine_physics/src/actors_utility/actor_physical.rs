use crate::actors::ActorPhysicalConfiguration;

#[derive(bevy_ecs::component::Component, Copy, Clone)]
pub(crate) struct ActorPhysical(pub ActorPhysicalConfiguration);
