// Copyright Rob Gage 2026

use egui::{Color32, Painter, Pos2, Rect, Shape, Vec2, pos2, vec2};

use super::{UiImageFit, UiTexture};

/// A retained texture region with independent fitting and tint settings
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiImage {
    /// The retained GPU texture metadata
    pub texture: UiTexture,
    /// The normalized source region in the texture
    pub uv: Rect,
    /// Source texels to inset from each sampling edge; defaults to `0.5`
    pub uv_inset: f32,
    /// The destination fitting policy
    pub fit: UiImageFit,
    /// The source color multiplier
    pub tint: Color32,
}

impl UiImage {

    /// Creates an image using the complete texture at native size
    pub fn new(texture: UiTexture) -> Self {
        Self {
            texture,
            uv: Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
            uv_inset: 0.5,
            fit: UiImageFit::Native,
            tint: Color32::WHITE,
        }
    }

    /// Creates an image from a retained egui texture handle
    pub fn from_handle(handle: &egui::TextureHandle) -> Self {
        Self::new(UiTexture::from_handle(handle))
    }

    /// Creates an image from explicit texture metadata
    pub const fn from_texture(texture: UiTexture) -> Self {
        Self {
            texture,
            uv: Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
            uv_inset: 0.5,
            fit: UiImageFit::Native,
            tint: Color32::WHITE,
        }
    }

    /// Sets the normalized source region
    pub const fn with_uv(mut self, uv: Rect) -> Self {
        self.uv = uv;
        self
    }

    /// Sets the texel-center inset used for draw sampling; zero preserves raw UVs
    pub const fn with_uv_inset(mut self, uv_inset: f32) -> Self {
        self.uv_inset = uv_inset;
        self
    }

    /// Sets the destination fitting policy
    pub const fn with_fit(mut self, fit: UiImageFit) -> Self {
        self.fit = fit;
        self
    }

    /// Sets the source color multiplier
    pub const fn with_tint(mut self, tint: Color32) -> Self {
        self.tint = tint;
        self
    }

    /// Paints this image into a destination rectangle
    pub fn paint(&self, painter: &Painter, rect: Rect) {
        self.paint_with_tint(painter, rect, self.tint)
    }

    /// Paints this image with an additional visual color multiplier
    pub(crate) fn paint_with_tint(&self, painter: &Painter, rect: Rect, tint: Color32) {
        painter.add(self.shape_with_tint(rect, tint));
    }

    pub(crate) fn shape(&self, rect: Rect) -> Shape {
        self.shape_with_tint(rect, self.tint)
    }

    pub(crate) fn shape_with_tint(&self, rect: Rect, tint: Color32) -> Shape {
        let source_size: Vec2 = self.source_size();
        if !is_positive_size(source_size) || !is_positive_size(rect.size()) {
            return Shape::Noop;
        }
        let (destination_rect, destination_uv): (Rect, Rect) = match self.fit {
            UiImageFit::Native => (Rect::from_center_size(rect.center(), source_size), self.uv),
            UiImageFit::Stretch => (rect, self.uv),
            UiImageFit::Contain => (contained_rect(rect, source_size), self.uv),
            UiImageFit::Cover => (rect, covered_uv(rect, source_size, self.uv)),
        };
        let Some((destination_rect, destination_uv)) = clip_image_geometry(destination_rect, destination_uv, rect) else {
            return Shape::Noop;
        };
        Shape::image(self.texture.id, destination_rect, self.sample_uv(destination_uv), tint)
    }

    /// Returns the source dimensions represented by the current UV region
    pub(crate) fn source_size(&self) -> Vec2 {
        let uv_size: Vec2 = self.uv.size();
        vec2(
            self.texture.pixel_size.x * uv_size.x.abs(),
            self.texture.pixel_size.y * uv_size.y.abs(),
        )
    }

    /// Returns draw UVs inset without changing the image's source geometry.
    pub(crate) fn sample_uv(&self, uv: Rect) -> Rect {
        inset_uv(uv, self.texture.pixel_size, self.uv_inset)
    }
}

pub(crate) fn inset_uv(uv: Rect, texture_size: Vec2, inset_pixels: f32) -> Rect {
    let inset_pixels: f32 = if inset_pixels.is_finite() {
        inset_pixels.max(0.0)
    } else {
        0.0
    };
    let (min_x, max_x): (f32, f32) = inset_axis(
        uv.min.x,
        uv.max.x,
        texture_size.x,
        inset_pixels,
    );
    let (min_y, max_y): (f32, f32) = inset_axis(
        uv.min.y,
        uv.max.y,
        texture_size.y,
        inset_pixels,
    );
    Rect::from_min_max(pos2(min_x, min_y), pos2(max_x, max_y))
}

fn inset_axis(start: f32, end: f32, texture_length: f32, inset_pixels: f32) -> (f32, f32) {
    if !start.is_finite() || !end.is_finite() || !texture_length.is_finite() || texture_length <= 0.0 {
        return (start, end);
    }
    let inset: f32 = (inset_pixels / texture_length).min((end - start).abs() * 0.5);
    if end >= start {
        (start + inset, end - inset)
    } else {
        (start - inset, end + inset)
    }
}

fn contained_rect(rect: Rect, source_size: Vec2) -> Rect {
    let scale: f32 = (rect.width() / source_size.x).min(rect.height() / source_size.y);
    Rect::from_center_size(rect.center(), source_size * scale)
}

fn covered_uv(rect: Rect, source_size: Vec2, uv: Rect) -> Rect {
    let scale: f32 = (rect.width() / source_size.x).max(rect.height() / source_size.y);
    let painted_size: Vec2 = source_size * scale;
    let visible_fraction_x: f32 = (rect.width() / painted_size.x).clamp(0.0, 1.0);
    let visible_fraction_y: f32 = (rect.height() / painted_size.y).clamp(0.0, 1.0);
    let cropped_x: (f32, f32) = centered_uv_axis(uv.min.x, uv.max.x, visible_fraction_x);
    let cropped_y: (f32, f32) = centered_uv_axis(uv.min.y, uv.max.y, visible_fraction_y);
    Rect::from_min_max(pos2(cropped_x.0, cropped_y.0), pos2(cropped_x.1, cropped_y.1))
}

fn centered_uv_axis(start: f32, end: f32, visible_fraction: f32) -> (f32, f32) {
    let crop_fraction: f32 = (1.0 - visible_fraction).clamp(0.0, 1.0);
    let delta: f32 = (end - start) * crop_fraction * 0.5;
    (start + delta, end - delta)
}

fn clip_image_geometry(destination: Rect, source: Rect, clip: Rect) -> Option<(Rect, Rect)> {
    if !destination.intersects(clip) {
        return None;
    }
    let clipped: Rect = destination.intersect(clip);
    let source_x: (f32, f32) = clipped_uv_axis(destination.left(), destination.right(), source.min.x, source.max.x, clipped.left(), clipped.right());
    let source_y: (f32, f32) = clipped_uv_axis(destination.top(), destination.bottom(), source.min.y, source.max.y, clipped.top(), clipped.bottom());
    Some((clipped, Rect::from_min_max(pos2(source_x.0, source_y.0), pos2(source_x.1, source_y.1))))
}

fn clipped_uv_axis(destination_start: f32, destination_end: f32, source_start: f32, source_end: f32, clipped_start: f32, clipped_end: f32) -> (f32, f32) {
    let destination_length: f32 = destination_end - destination_start;
    if !destination_length.is_finite() || destination_length <= 0.0 {
        return (source_start, source_end);
    }
    let start_fraction: f32 = ((clipped_start - destination_start) / destination_length).clamp(0.0, 1.0);
    let end_fraction: f32 = ((clipped_end - destination_start) / destination_length).clamp(0.0, 1.0);
    (source_start + (source_end - source_start) * start_fraction, source_start + (source_end - source_start) * end_fraction)
}

fn is_positive_size(size: Vec2) -> bool {
    size.x.is_finite() && size.y.is_finite() && size.x > 0.0 && size.y > 0.0
}
