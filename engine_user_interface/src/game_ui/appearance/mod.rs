// Copyright Rob Gage 2026

//! Paint-only appearance primitives for the independent game UI

mod ui_appearance;
mod ui_brush;
mod ui_image;
mod ui_image_fit;
mod ui_insets;
mod ui_nine_slice;
mod ui_nine_slice_mode;
mod ui_texture;
mod ui_visual;

pub use ui_appearance::UiAppearance;
pub use ui_brush::UiBrush;
pub use ui_image::UiImage;
pub use ui_image_fit::UiImageFit;
pub use ui_insets::UiInsets;
pub use ui_nine_slice::UiNineSlice;
pub use ui_nine_slice_mode::UiNineSliceMode;
pub use ui_texture::UiTexture;
pub use ui_visual::UiVisual;
