use super::Actor;

/// Whether two actors have just begun or ended touching.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ActorContactState {
    Started,
    Ended,
}

/// A deduplicated logical actor-to-actor contact change.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct ActorContactEvent {
    pub first: Actor,
    pub second: Actor,
    pub state: ActorContactState,
}
