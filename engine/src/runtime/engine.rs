// Copyright Rob Gage 2026

use super::EngineError;
use crate::games::{
    Game,
    GameInstance
};

/// An engine runtime to be used by `Game`s
pub struct Engine {

}

impl Engine {

    /// Launches a game using this `Engine`
    pub fn launch<G: Game>(self, game: G) -> Result<(), EngineError> {
        let game_instance: GameInstance<G> = GameInstance::new(self, game);
        Ok(())
    }

}