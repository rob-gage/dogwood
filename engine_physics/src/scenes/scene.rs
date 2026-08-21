// Copyright Rob Gage 2026

/// A scene that can be simulated by the engine
pub struct Scene<
    const WIDTH: usize,
    const HEIGHT: usize,
> {
    chunks: [[(); HEIGHT]; WIDTH],
}