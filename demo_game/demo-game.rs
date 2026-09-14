// Copyright Rob Gage 2026

use demo_game::DemoGame;
use engine::{
    Game,
    compute::Accelerator,
};
use std::sync::Arc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _tracing_guard = engine::diagnostics::initialize();
    let accelerator: Arc<Accelerator> = Arc::new(Accelerator::new()?);
    DemoGame::new(&accelerator)?.launch(accelerator)
}
