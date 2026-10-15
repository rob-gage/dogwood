// Copyright Rob Gage 2026

//! Taffy-backed game UI layout styles

use super::UiLength;

/// A layout style and its clipping policy
#[derive(Clone, Debug, PartialEq)]
pub struct UiLayout {
    /// Taffy style properties
    pub style: taffy::Style,
    /// Whether descendants should be clipped by this node
    pub clip: bool,
    /// Width flex intent supplied through [`Self::with_width`].
    ///
    /// This is kept separate from Taffy's axis-independent `flex_grow` field so
    /// the layout engine can apply it to the parent's actual main axis. An
    /// explicit raw dimension or non-default raw flex basis/grow value takes
    /// precedence over this sidecar during normalization.
    pub desired_width: Option<UiLength>,
    /// Height flex intent supplied through [`Self::with_height`].
    ///
    /// Cross-axis flex intent falls back to cross-axis fill because flex grow has
    /// no cross-axis meaning. An explicit raw dimension takes precedence over
    /// this sidecar during normalization.
    pub desired_height: Option<UiLength>,
}

impl Default for UiLayout {

    fn default() -> Self {
        let mut style: taffy::Style = taffy::Style::default();
        style.display = taffy::Display::Flex;
        style.flex_direction = taffy::FlexDirection::Column;
        Self { style, clip: false, desired_width: None, desired_height: None }
    }
}

impl UiLayout {

    /// Creates the default column flex layout
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the width constraint
    #[must_use]
    pub fn with_width(mut self, length: impl Into<UiLength>) -> Self {
        let length: UiLength = length.into();
        self.style.size.width = length.to_dimension();
        self.desired_width = length.flex_weight().map(|_| length);
        self
    }

    /// Sets the height constraint
    #[must_use]
    pub fn with_height(mut self, length: impl Into<UiLength>) -> Self {
        let length: UiLength = length.into();
        self.style.size.height = length.to_dimension();
        self.desired_height = length.flex_weight().map(|_| length);
        self
    }

    /// Sets the minimum width constraint
    #[must_use]
    pub fn with_min_width(mut self, length: impl Into<UiLength>) -> Self {
        let length: UiLength = length.into();
        self.style.min_size.width = length.to_length_percentage_auto();
        self
    }

    /// Sets the minimum height constraint
    #[must_use]
    pub fn with_min_height(mut self, length: impl Into<UiLength>) -> Self {
        let length: UiLength = length.into();
        self.style.min_size.height = length.to_length_percentage_auto();
        self
    }

    /// Sets the maximum width constraint
    #[must_use]
    pub fn with_max_width(mut self, length: impl Into<UiLength>) -> Self {
        let length: UiLength = length.into();
        self.style.max_size.width = length.to_length_percentage_auto();
        self
    }

    /// Sets the maximum height constraint
    #[must_use]
    pub fn with_max_height(mut self, length: impl Into<UiLength>) -> Self {
        let length: UiLength = length.into();
        self.style.max_size.height = length.to_length_percentage_auto();
        self
    }

    /// Sets all padding edges
    #[must_use]
    pub fn with_padding(mut self, length: impl Into<UiLength>) -> Self {
        let length: UiLength = length.into();
        let value: taffy::LengthPercentage = length.to_length_percentage();
        self.style.padding = taffy::Rect { left: value, right: value, top: value, bottom: value };
        self
    }

    /// Sets all margin edges
    #[must_use]
    pub fn with_margin(mut self, length: impl Into<UiLength>) -> Self {
        let length: UiLength = length.into();
        let value: taffy::LengthPercentageAuto = length.to_length_percentage_auto();
        self.style.margin = taffy::Rect { left: value, right: value, top: value, bottom: value };
        self
    }

    /// Sets both row and column gaps
    #[must_use]
    pub fn with_gap(mut self, length: impl Into<UiLength>) -> Self {
        let length: UiLength = length.into();
        let value: taffy::LengthPercentage = length.to_length_percentage();
        self.style.gap = taffy::Size { width: value, height: value };
        self
    }

    /// Sets the preferred aspect ratio
    #[must_use]
    pub fn with_aspect_ratio(mut self, ratio: f32) -> Self {
        self.style.aspect_ratio = ratio.is_finite().then_some(ratio).filter(|value: &f32| *value > 0.0);
        self
    }

    /// Sets whether descendants are clipped by this node
    #[must_use]
    pub fn with_clip(mut self, clip: bool) -> Self {
        self.clip = clip;
        self
    }

    /// Replaces the complete Taffy style
    #[must_use]
    pub fn with_style(mut self, style: taffy::Style) -> Self {
        self.style = style;
        self.desired_width = None;
        self.desired_height = None;
        self
    }

    /// Sets the padding edges directly
    #[must_use]
    pub fn with_padding_rect(mut self, padding: taffy::Rect<taffy::LengthPercentage>) -> Self {
        self.style.padding = padding;
        self
    }

    /// Sets the margin edges directly
    #[must_use]
    pub fn with_margin_rect(mut self, margin: taffy::Rect<taffy::LengthPercentageAuto>) -> Self {
        self.style.margin = margin;
        self
    }

    /// Sets the horizontal and vertical gaps independently
    #[must_use]
    pub fn with_gap_xy(
        mut self,
        horizontal: impl Into<UiLength>,
        vertical: impl Into<UiLength>,
    ) -> Self {
        let horizontal: UiLength = horizontal.into();
        let vertical: UiLength = vertical.into();
        self.style.gap = taffy::Size {
            width: horizontal.to_length_percentage(),
            height: vertical.to_length_percentage(),
        };
        self
    }

    /// Sets the Taffy flex direction
    #[must_use]
    pub fn with_flex_direction(mut self, direction: taffy::FlexDirection) -> Self {
        self.style.flex_direction = direction;
        self
    }

}
