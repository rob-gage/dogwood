// Copyright Rob Gage 2026

/// Runtime settings for the native game window
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowConfiguration {
    /// Title displayed by the native window
    pub title: String,
    /// Maximum render frames per second, with `u32::MAX` representing an uncapped rate
    pub fps_limit: u32,
}

impl Default for WindowConfiguration {

    fn default() -> Self {
        WindowConfiguration {
            title: String::from("Dogwood"),
            fps_limit: u32::MAX,
        }
    }

}
