// Copyright Rob Gage 2026

mod game;

use game::DemoGame;
use engine::games::Game;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    DemoGame::new().launch()
}
