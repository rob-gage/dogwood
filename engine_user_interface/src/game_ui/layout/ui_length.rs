// Copyright Rob Gage 2026

//! Logical lengths used by game UI layout styles

/// A logical UI length
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum UiLength {
    /// Let the layout algorithm determine the size
    Auto,
    /// A fixed logical size
    Fixed(f32),
    /// A percentage of the containing block, expressed from zero to one hundred
    Percent(f32),
    /// Fill available flex space
    Fill,
    /// Fill available flex space using the supplied weight
    Flex(f32),
}

impl Default for UiLength {

    fn default() -> Self {
        Self::Auto
    }
}

impl From<f32> for UiLength {

    fn from(value: f32) -> Self {
        Self::Fixed(value)
    }
}

impl UiLength {

    /// Creates a clamped percentage length
    #[must_use]
    pub fn percent(value: f32) -> Self {
        let safe_value: f32 = if value.is_finite() { value.clamp(0.0, 100.0) } else { 0.0 };
        Self::Percent(safe_value)
    }

    /// Creates a non-negative fixed length
    #[must_use]
    pub fn fixed(value: f32) -> Self {
        let safe_value: f32 = if value.is_finite() { value.max(0.0) } else { 0.0 };
        Self::Fixed(safe_value)
    }

    /// Creates a flex length with a non-negative weight
    #[must_use]
    pub fn flex(value: f32) -> Self {
        let safe_value: f32 = if value.is_finite() { value.max(0.0) } else { 0.0 };
        Self::Flex(safe_value)
    }

    pub(crate) fn to_dimension(self) -> taffy::Dimension {
        match self {
            Self::Auto | Self::Fill | Self::Flex(_) => taffy::Dimension::auto(),
            Self::Fixed(value) => taffy::Dimension::length(sanitize_non_negative(value)),
            Self::Percent(value) => taffy::Dimension::percent(sanitize_percent(value)),
        }
    }

    pub(crate) fn to_length_percentage(self) -> taffy::LengthPercentage {
        match self {
            Self::Percent(value) => taffy::LengthPercentage::percent(sanitize_percent(value)),
            Self::Fixed(value) => taffy::LengthPercentage::length(sanitize_non_negative(value)),
            Self::Auto | Self::Fill | Self::Flex(_) => taffy::LengthPercentage::length(0.0),
        }
    }

    pub(crate) fn to_length_percentage_auto(self) -> taffy::LengthPercentageAuto {
        match self {
            Self::Auto | Self::Fill | Self::Flex(_) => taffy::LengthPercentageAuto::auto(),
            Self::Fixed(value) => taffy::LengthPercentageAuto::length(sanitize_non_negative(value)),
            Self::Percent(value) => taffy::LengthPercentageAuto::percent(sanitize_percent(value)),
        }
    }

    pub(crate) fn flex_weight(self) -> Option<f32> {
        match self {
            Self::Fill => Some(1.0),
            Self::Flex(value) => Some(sanitize_non_negative(value)),
            Self::Auto | Self::Fixed(_) | Self::Percent(_) => None,
        }
    }
}

fn sanitize_non_negative(value: f32) -> f32 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}

fn sanitize_percent(value: f32) -> f32 {
    sanitize_non_negative(value).min(100.0) / 100.0
}
