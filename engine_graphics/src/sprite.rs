// Copyright Rob Gage 2026

use super::Color;

/// A sprite
pub struct Sprite<
    const WIDTH: usize,
    const HEIGHT: usize,
>([[(); HEIGHT]; WIDTH]);