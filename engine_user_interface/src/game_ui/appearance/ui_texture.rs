// Copyright Rob Gage 2026

use egui::{TextureHandle, TextureId, Vec2};

/// Metadata for an egui texture; the application retains any originating handle
///
/// The application retains the originating handle and chooses its `TextureOptions`, including nearest sampling, once during upload.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiTexture {
    /// The egui texture registered by the application or renderer
    pub id: TextureId,
    /// The dimensions of the complete texture in source pixels
    pub pixel_size: Vec2,
}

impl UiTexture {

    /// Creates metadata for a texture already registered with egui
    pub const fn new(id: TextureId, pixel_size: Vec2) -> Self {
        Self { id, pixel_size }
    }

    /// Creates metadata for an explicitly supplied GPU texture identifier
    pub const fn from_id(id: TextureId, pixel_size: Vec2) -> Self {
        Self::new(id, pixel_size)
    }

    /// Creates metadata from a retained egui texture handle
    pub fn from_handle(handle: &TextureHandle) -> Self {
        Self {
            id: handle.id(),
            pixel_size: handle.size_vec2(),
        }
    }

    /// Returns the texture identifier used by the painter
    pub const fn texture_id(self) -> TextureId {
        self.id
    }

    /// Returns the complete source texture size in pixels
    pub const fn intrinsic_size(self) -> Vec2 {
        self.pixel_size
    }
}
