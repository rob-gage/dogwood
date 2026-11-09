// Copyright Rob Gage 2026

use super::WindowMode;

/// Runtime settings for the native game window
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowConfiguration {
    /// Title displayed by the native window
    pub title: String,
    /// Maximum render frames per second, with `u32::MAX` representing an uncapped rate
    pub fps_limit: u32,
    /// How the window occupies the screen when it opens, windowed by default; the game may change
    /// it while running through `Game::window_mode`
    pub mode: WindowMode,
}

impl Default for WindowConfiguration {

    fn default() -> Self {
        WindowConfiguration {
            title: String::from("Dogwood"),
            fps_limit: u32::MAX,
            mode: WindowMode::Windowed,
        }
    }

}
