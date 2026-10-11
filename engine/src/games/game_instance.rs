// Copyright Rob Gage 2026

use super::Game;
use crate::runtime::Engine;

/// An instance of a `Game` running on the `Engine`
pub struct GameInstance<G: Game> {
    /// The engine being used to run the `Game`
    _engine: Engine,
    /// The `Game` running in this `GameInstance`
    game: G,
}

impl<G: Game> GameInstance<G> {

    /// Creates a new `GameInstance` from an `Engine` and a `Game`
    pub const fn new(engine: Engine, game: G) -> GameInstance<G> {
        GameInstance { _engine: engine, game }
    }

    /// Separates the running engine from its game for host event dispatch
    pub fn into_parts(self) -> (Engine, G) { (self._engine, self.game) }

}
