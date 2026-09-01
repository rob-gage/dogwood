// Copyright Rob Gage 2026

mod game;

use game::DemoGame;
#[cfg(not(feature = "editor"))]
use engine::games::Game;
#[cfg(feature = "editor")]
use editor::EditorGame;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(feature = "editor")]
    return DemoGame::new().launch_in_editor();

    #[cfg(not(feature = "editor"))]
    DemoGame::new().launch()
}
