// Copyright Rob Gage 2026

use demo_game::DemoGame;
use editor::EditorGame;

fn main() -> Result<(), Box<dyn std::error::Error>> { DemoGame::new().launch_in_editor() }
