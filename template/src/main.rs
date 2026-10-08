// Copyright Rob Gage 2026

use engine::{
    Accelerator,
    games::Game,
    runtime::Engine,
};

struct MyGame;

impl Game for MyGame {}

fn main() -> Result<(), engine::runtime::EngineError> {
    let accelerator: Accelerator = Accelerator::new()?;
    Engine::new(accelerator).launch(MyGame)
}
