// Copyright Rob Gage 2026

//! Component variants stored by a game UI element

use super::super::{UiBrush, UiImage};
use std::borrow::Cow;
use std::ops::RangeInclusive;

/// The concrete rendering and interaction behavior of one UI element
pub enum UiComponentKind<'a> {
    /// A visual container
    Panel,
    /// A horizontal flex container
    Row,
    /// A vertical flex container
    Column,
    /// A container whose children are painted in the same coordinate space
    Overlay,
    /// A CSS grid container
    Grid,
    /// A native egui scrolling viewport
    Scroll,
    /// A flexible empty region
    Spacer { flex: f32 },
    /// A measured and painted text node
    Text(Cow<'a, str>),
    /// A retained image node
    Image(UiImage),
    /// A visual brush node
    Rectangle(UiBrush),
    /// A one-pixel separator
    Separator { foreground: Option<UiBrush> },
    /// A normalized progress indicator
    Progress { value: f32, foreground: UiBrush },
    /// A custom painted sprite button
    Button { label: Cow<'a, str> },
    /// A custom painted image button
    ImageButton { image: UiImage },
    /// A borrowed boolean control
    Toggle { label: Cow<'a, str>, value: &'a mut bool },
    /// A borrowed floating-point control
    Slider { value: &'a mut f32, range: RangeInclusive<f32> },
    /// A borrowed editable string control
    TextInput { value: &'a mut String },
}
