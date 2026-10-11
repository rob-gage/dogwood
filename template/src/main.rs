// Copyright Rob Gage 2026

use engine::{
    Accelerator,
    Processor,
    games::Game,
    runtime::Engine,
};

struct MyGame;

impl Game for MyGame {}

fn main() -> Result<(), engine::runtime::EngineError> {
    let accelerator: Accelerator = Accelerator::default();
    let processor: Processor = Processor::default();
    Engine::new(accelerator, processor).launch(MyGame)
}
