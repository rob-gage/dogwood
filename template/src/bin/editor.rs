// Copyright Rob Gage 2026

use editor::Editor;
use engine::runtime::{
    Engine,
    EngineError,
};
use {{crate_name}}::TemplateGame;

fn main() -> Result<(), EngineError> {
    let engine: Engine = Engine::default();
    let game: TemplateGame = TemplateGame::new(engine.accelerator())?;
    Editor::launch(engine, game)
}
