// Copyright Rob Gage 2026

use engine::{
    games::Game,
    runtime::Engine,
};

struct MyGame;

impl Game for MyGame {}

fn main() -> Result<(), engine::runtime::EngineError> {
    Engine {}.launch(MyGame)
}
