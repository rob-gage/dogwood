// Copyright Rob Gage 2026

use demo_game::DemoGame;
use engine::compute::Accelerator;
use editor::EditorGame;
use std::sync::Arc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let accelerator: Arc<Accelerator> = Arc::new(Accelerator::new()?);
    DemoGame::new(&accelerator)?.launch_in_editor(accelerator)
}
