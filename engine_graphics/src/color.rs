// Copyright Rob Gage 2026

/// A color with red, green, blue, and alpha channels
#[derive(Copy, Clone)]
pub struct Color(u32);

impl Color {
    /// The color black
    pub const BLACK: Self = Self::new_rgb(0, 0, 0);

    /// Creates a color from red, green, and blue components
    pub const fn new_rgb(red: u8, green: u8, blue: u8) -> Color {
        Self::new_rgba(red, green, blue, 255)
    }

    /// Creates a color from red, green, blue, and alpha components
    pub const fn new_rgba(red: u8, green: u8, blue: u8, alpha: u8) -> Self {
        Self(red as u32 | (green as u32) << 8 | (blue as u32) << 16 | (alpha as u32) << 24)
    }

    /// Returns the red component of this `Color`
    pub const fn red(self) -> u8 {
        self.0 as u8
    }

    /// Returns the green component of this `Color`
    pub const fn green(self) -> u8 {
        (self.0 >> 8) as u8
    }

    /// Returns the blue component of this `Color`
    pub const fn blue(self) -> u8 {
        (self.0 >> 16) as u8
    }

    /// Returns the alpha component of this `Color`
    pub const fn alpha(self) -> u8 {
        (self.0 >> 24) as u8
    }

    /// Returns this `Color` as a packed RGBA `u32`
    pub const fn as_u32(self) -> u32 {
        self.0
    }
}

impl From<&Color> for egui::Color32 {
    fn from(color: &Color) -> Self {
        egui::Color32::from_rgba_unmultiplied(
            color.red(),
            color.green(),
            color.blue(),
            color.alpha(),
        )
    }
}
