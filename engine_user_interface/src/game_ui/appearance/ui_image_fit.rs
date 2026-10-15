// Copyright Rob Gage 2026

/// Controls how an image occupies its destination rectangle
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UiImageFit {
    /// Paint at source size, centered in the destination
    #[default]
    Native,
    /// Stretch the source region to the complete destination
    Stretch,
    /// Preserve the source aspect ratio without cropping
    Contain,
    /// Preserve the source aspect ratio while covering and cropping
    Cover,
}
