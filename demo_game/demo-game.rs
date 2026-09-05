// Copyright Rob Gage 2026

use demo_game::DemoGame;
use engine::{
    Game,
    compute::Accelerator,
};
use std::sync::Arc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let accelerator: Arc<Accelerator> = Arc::new(pollster::block_on(Accelerator::new())?);
    DemoGame::new(&accelerator)?.launch(accelerator)
}
