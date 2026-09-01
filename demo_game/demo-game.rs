// Copyright Rob Gage 2026

use demo_game::DemoGame;
use engine::games::Game;

fn main() -> Result<(), Box<dyn std::error::Error>> { DemoGame::new().launch() }
