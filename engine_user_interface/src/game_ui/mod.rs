// Copyright Rob Gage 2026

//! Constraint-based game components sharing the native egui context and renderer

pub mod layout;
pub mod appearance;
pub mod components;
mod ui_fonts;
mod ui_text_style;
mod ui_responses;

pub use components::{GameUi, UiElement};
pub use layout::{UiLength, UiLayout};
pub use appearance::{
    UiAppearance,
    UiBrush,
    UiImage,
    UiImageFit,
    UiInsets,
    UiNineSlice,
    UiNineSliceMode,
    UiTexture,
    UiVisual,
};
pub use ui_fonts::UiFonts;
pub use ui_text_style::UiTextStyle;
pub use ui_responses::UiResponses;
pub use taffy;
