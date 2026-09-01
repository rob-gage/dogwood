// Copyright Rob Gage 2026

/// A color with red, green, blue, and alpha channels
pub struct Color {
    /// The amount of red in this `Color` (between `0` and `1`)
    pub red: f32,
    /// The amount of green in this `Color` (between `0` and `1`)
    pub green: f32,
    /// The amount of blue in this `Color` (between `0` and `1`)
    pub blue: f32,
    /// How opaque this `Color` is (`0` is fully translucent, `1` is fully opaque`)
    pub alpha: f32,
}

impl Into<egui::Color32> for &Color {

    fn into(self) -> egui::Color32 {
        egui::Color32::from_rgba_unmultiplied(
            (self.red * 255.0) as u8,
            (self.green * 255.0) as u8,
            (self.blue * 255.0) as u8,
            (self.alpha * 255.0) as u8,
        )
    }

}