// Copyright Rob Gage 2026

mod template_game;
mod template_scene_generator;

use engine::runtime::{
    Engine,
    EngineError,
};
use template_game::TemplateGame;

fn main() -> Result<(), EngineError> {
    let engine: Engine = Engine::default();
    let game: TemplateGame = TemplateGame::new(engine.accelerator())?;
    engine.launch(game)
}
