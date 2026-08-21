// Copyright Rob Gage 2026

use engine::games::Game;
use std::error::Error;

pub struct DemoGame;

impl Game for DemoGame {

    const TITLE: &'static str = "Dogwood Demo Game";

}

fn main() -> Result<(), Box<dyn Error>> {
    let game: DemoGame = DemoGame;
    game.launch()
}
