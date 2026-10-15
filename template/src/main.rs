// Copyright Rob Gage 2026

use engine::runtime::{
    Engine,
    EngineError,
};
use {{crate_name}}::TemplateGame;

fn main() -> Result<(), EngineError> {
    let engine: Engine = Engine::default();
    let game: TemplateGame = TemplateGame::new(engine.accelerator())?;
    engine.launch(game)
}
