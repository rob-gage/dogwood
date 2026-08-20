// Copyright Rob Gage 2026

use std::error::Error;

/// Implementors are games that run on this engine
pub trait Game {

    /// Launches this `Game`
    fn launch(self) -> Result<(), Box<dyn Error>>;

}