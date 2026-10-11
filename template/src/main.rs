// Copyright Rob Gage 2026

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
    Engine::new(accelerator, processor).launch(TemplateGame::new())
}
