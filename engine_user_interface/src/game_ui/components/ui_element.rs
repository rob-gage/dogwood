// Copyright Rob Gage 2026

//! Owned game UI descriptions with borrowed application control state

use super::UiComponentKind;
use crate::game_ui::{UiAppearance, UiBrush, UiImage, UiLayout, UiLength, UiTextStyle};
use egui::Id;
use std::borrow::Cow;
use std::fmt::Debug;
use std::hash::Hash;
use std::ops::RangeInclusive;

/// An owned immediate-mode element description
pub struct UiElement<'a> {
    pub(crate) kind: UiComponentKind<'a>,
    pub(crate) layout: UiLayout,
    pub(crate) appearance: UiAppearance,
    pub(crate) typography: UiTextStyle,
    pub(crate) enabled: bool,
    pub(crate) selected: bool,
    pub(crate) explicit_id: Option<Id>,
    pub(crate) children: Vec<UiElement<'a>>,
}

impl<'a> UiElement<'a> {

    /// Creates a panel container
    pub fn panel() -> Self {
        Self::container(UiComponentKind::Panel)
    }

    /// Creates a horizontal flex container
    pub fn row() -> Self {
        let mut element: Self = Self::container(UiComponentKind::Row);
        element.layout = element.layout.with_flex_direction(taffy::FlexDirection::Row);
        element
    }

    /// Creates a vertical flex container
    pub fn column() -> Self {
        Self::container(UiComponentKind::Column)
    }

    /// Creates a paint-overlaid container
    pub fn overlay() -> Self {
        Self::container(UiComponentKind::Overlay)
    }

    /// Creates a grid container with evenly sized columns
    pub fn grid(columns: usize) -> Self {
        let mut element: Self = Self::container(UiComponentKind::Grid);
        element.layout.style.display = taffy::Display::Grid;
        element.layout.style.grid_template_columns = taffy::style_helpers::evenly_sized_tracks(columns.max(1) as u16);
        element
    }

    /// Creates a native scrolling container
    pub fn scroll() -> Self {
        let mut element: Self = Self::container(UiComponentKind::Scroll);
        element.layout.clip = true;
        element
    }

    /// Creates an empty flexible region
    pub fn spacer(flex: f32) -> Self {
        Self::container(UiComponentKind::Spacer { flex })
    }

    /// Creates a text node
    pub fn text(text: impl Into<Cow<'a, str>>) -> Self {
        Self::leaf(UiComponentKind::Text(text.into()))
    }

    /// Creates an image node
    pub fn image(image: UiImage) -> Self {
        Self::leaf(UiComponentKind::Image(image))
    }

    /// Creates a brush-painted rectangle
    pub fn rectangle(brush: UiBrush) -> Self {
        Self::leaf(UiComponentKind::Rectangle(brush))
    }

    /// Creates a one-pixel separator
    pub fn separator() -> Self {
        Self::leaf(UiComponentKind::Separator { foreground: None })
    }

    /// Creates a normalized progress indicator
    pub fn progress(value: f32) -> Self {
        Self::leaf(UiComponentKind::Progress {
            value,
            foreground: UiBrush::Color(egui::Color32::from_rgb(80, 180, 110)),
        })
    }

    /// Creates a custom painted button with a stable ID
    pub fn button(id: impl Hash + Debug, label: impl Into<Cow<'a, str>>) -> Self {
        Self::with_id(UiComponentKind::Button { label: label.into() }, id)
    }

    /// Creates a custom painted image button with a stable ID
    pub fn image_button(id: impl Hash + Debug, image: UiImage) -> Self {
        Self::with_id(UiComponentKind::ImageButton { image }, id)
    }

    /// Creates a borrowed boolean control with a stable ID
    pub fn toggle(id: impl Hash + Debug, label: impl Into<Cow<'a, str>>, value: &'a mut bool) -> Self {
        Self::with_id(UiComponentKind::Toggle { label: label.into(), value }, id)
    }

    /// Creates a borrowed floating-point slider with a stable ID
    pub fn slider(id: impl Hash + Debug, value: &'a mut f32, range: RangeInclusive<f32>) -> Self {
        Self::with_id(UiComponentKind::Slider { value, range }, id)
    }

    /// Creates a borrowed single-line text input with a stable ID
    pub fn text_input(id: impl Hash + Debug, value: &'a mut String) -> Self {
        Self::with_id(UiComponentKind::TextInput { value }, id)
    }

    /// Sets the preferred width
    pub fn width(mut self, length: impl Into<UiLength>) -> Self {
        self.layout = self.layout.with_width(length);
        self
    }

    /// Sets the preferred height
    pub fn height(mut self, length: impl Into<UiLength>) -> Self {
        self.layout = self.layout.with_height(length);
        self
    }

    /// Sets the minimum width
    pub fn min_width(mut self, length: impl Into<UiLength>) -> Self {
        self.layout = self.layout.with_min_width(length);
        self
    }

    /// Sets the minimum height
    pub fn min_height(mut self, length: impl Into<UiLength>) -> Self {
        self.layout = self.layout.with_min_height(length);
        self
    }

    /// Sets the maximum width
    pub fn max_width(mut self, length: impl Into<UiLength>) -> Self {
        self.layout = self.layout.with_max_width(length);
        self
    }

    /// Sets the maximum height
    pub fn max_height(mut self, length: impl Into<UiLength>) -> Self {
        self.layout = self.layout.with_max_height(length);
        self
    }

    /// Sets uniform padding
    pub fn padding(mut self, length: impl Into<UiLength>) -> Self {
        self.layout = self.layout.with_padding(length);
        self
    }

    /// Sets uniform margin
    pub fn margin(mut self, length: impl Into<UiLength>) -> Self {
        self.layout = self.layout.with_margin(length);
        self
    }

    /// Sets uniform child gap
    pub fn gap(mut self, length: impl Into<UiLength>) -> Self {
        self.layout = self.layout.with_gap(length);
        self
    }

    /// Aligns children on the cross axis
    pub fn align(mut self, align: egui::Align) -> Self {
        self.layout.style.align_items = Some(match align {
            egui::Align::Min => taffy::AlignItems::START,
            egui::Align::Center => taffy::AlignItems::CENTER,
            egui::Align::Max => taffy::AlignItems::END,
        });
        self
    }

    /// Sets the main-axis distribution
    pub fn justify(mut self, justify: taffy::JustifyContent) -> Self {
        self.layout.style.justify_content = Some(justify);
        self
    }

    /// Replaces the layout style
    pub fn layout(mut self, layout: UiLayout) -> Self {
        self.layout = layout;
        self
    }

    /// Replaces all visual states
    pub fn appearance(mut self, appearance: UiAppearance) -> Self {
        self.appearance = appearance;
        self
    }

    /// Replaces the text style
    pub fn typography(mut self, typography: UiTextStyle) -> Self {
        self.typography = typography;
        self
    }

    /// Enables or disables interaction and native editing
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Sets the selected visual state
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// Sets whether descendants are clipped to this element's content box
    pub fn clip(mut self, clip: bool) -> Self {
        self.layout = self.layout.with_clip(clip);
        self
    }

    /// Sets the preferred width-to-height ratio
    pub fn aspect_ratio(mut self, ratio: f32) -> Self {
        self.layout = self.layout.with_aspect_ratio(ratio);
        self
    }

    /// Sets the flex grow factor
    pub fn grow(mut self, factor: f32) -> Self {
        self.layout.style.flex_grow = if factor.is_finite() { factor.max(0.0) } else { 0.0 };
        self
    }

    /// Sets the flex shrink factor
    pub fn shrink(mut self, factor: f32) -> Self {
        self.layout.style.flex_shrink = if factor.is_finite() { factor.max(0.0) } else { 0.0 };
        self
    }

    /// Sets the foreground brush used by progress and separator elements
    pub fn foreground(mut self, brush: UiBrush) -> Self {
        match &mut self.kind {
            UiComponentKind::Separator { foreground } => *foreground = Some(brush),
            UiComponentKind::Progress { foreground, .. } => *foreground = brush,
            _ => {}
        }
        self
    }

    /// Sets a stable global explicit ID.
    ///
    /// Explicit IDs are not scoped by [`GameUi`](super::GameUi); namespace the
    /// value at the call site when the same ID is used by multiple surfaces.
    pub fn id(mut self, id: impl Hash + Debug) -> Self {
        self.explicit_id = Some(Id::new(id));
        self
    }

    /// Appends one child without converting it through an intermediate type
    pub fn child(mut self, child: UiElement<'a>) -> Self {
        self.children.push(child);
        self
    }

    /// Appends all supplied children
    pub fn children<I>(mut self, children: I) -> Self
    where
        I: IntoIterator<Item = UiElement<'a>>,
    {
        self.children.extend(children);
        self
    }

    fn container(kind: UiComponentKind<'a>) -> Self {
        Self::new(kind)
    }

    fn leaf(kind: UiComponentKind<'a>) -> Self {
        Self::new(kind)
    }

    fn with_id(kind: UiComponentKind<'a>, id: impl Hash + Debug) -> Self {
        let mut element: Self = Self::new(kind);
        element.explicit_id = Some(Id::new(id));
        element
    }

    fn new(kind: UiComponentKind<'a>) -> Self {
        Self {
            kind,
            layout: UiLayout::default(),
            appearance: UiAppearance::default(),
            typography: UiTextStyle::default(),
            enabled: true,
            selected: false,
            explicit_id: None,
            children: Vec::new(),
        }
    }
}
