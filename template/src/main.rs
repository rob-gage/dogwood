// Copyright Rob Gage 2026

mod materials;
mod template_game;
mod template_scene_generator;

use engine::{
    Accelerator,
    Processor,
    runtime::{
        Engine,
        EngineError,
    },
};
use template_game::TemplateGame;

fn main() -> Result<(), EngineError> {
    let accelerator: Accelerator = Accelerator::default();
    let processor: Processor = Processor::default();
    let game: TemplateGame = TemplateGame::new(&accelerator);
    Engine::new(accelerator, processor).launch(game)
}
