// Copyright Rob Gage 2026

/// A mouse button
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct Button(pub(crate) winit::event::MouseButton);

impl Button {
    /// The left mouse button
    pub const LEFT: Self = Self(winit::event::MouseButton::Left);

    /// The right mouse button
    pub const RIGHT: Self = Self(winit::event::MouseButton::Right);

    /// The middle mouse button
    pub const MIDDLE: Self = Self(winit::event::MouseButton::Middle);
}
