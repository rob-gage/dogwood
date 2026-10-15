// Copyright Rob Gage 2026

use egui::{Color32, Painter, Rect, Shape, Vec2, pos2, vec2};

use super::{UiImage, UiInsets, UiNineSliceMode};

const MAX_TILES_PER_AXIS: usize = 64;

/// A nine-slice image with proportional small-rectangle-safe borders
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiNineSlice {
    /// The source image and its color tint
    pub image: UiImage,
    /// Source border distances in texture pixels
    pub borders: UiInsets,
    /// Multiplier applied to source border distances in destination points
    pub scale: f32,
    /// Stretch or tile the center region
    pub center: UiNineSliceMode,
    /// Stretch or tile the four edge regions
    pub edges: UiNineSliceMode,
}

impl UiNineSlice {

    /// Creates a stretched nine-slice with equal borders
    pub fn new(image: UiImage, borders: UiInsets) -> Self {
        Self {
            image,
            borders,
            scale: 1.0,
            center: UiNineSliceMode::Stretch,
            edges: UiNineSliceMode::Stretch,
        }
    }

    /// Sets the destination border scale
    pub const fn with_scale(mut self, scale: f32) -> Self {
        self.scale = scale;
        self
    }

    /// Sets the center rendering mode
    pub const fn with_center_mode(mut self, mode: UiNineSliceMode) -> Self {
        self.center = mode;
        self
    }

    /// Sets the edge rendering mode
    pub const fn with_edge_mode(mut self, mode: UiNineSliceMode) -> Self {
        self.edges = mode;
        self
    }

    /// Paints the nine-slice into a destination rectangle
    pub fn paint(&self, painter: &Painter, rect: Rect) {
        self.paint_with_tint(painter, rect, self.image.tint)
    }

    /// Paints the nine-slice with an additional visual color multiplier
    pub(crate) fn paint_with_tint(&self, painter: &Painter, rect: Rect, tint: Color32) {
        painter.add(self.shape_with_tint(rect, tint));
    }

    pub(crate) fn shape(&self, rect: Rect) -> Shape {
        self.shape_with_tint(rect, self.image.tint)
    }

    pub(crate) fn shape_with_tint(&self, rect: Rect, tint: Color32) -> Shape {
        let source_size: Vec2 = self.image.source_size();
        if !is_positive_size(source_size) || !is_positive_size(rect.size()) {
            return Shape::Noop;
        }
        let source_borders: UiInsets = self.safe_source_borders(source_size);
        let scaled_borders: UiInsets = self.safe_destination_borders(rect, source_borders);
        let destination_x: [f32; 4] = [
            rect.left(),
            rect.left() + scaled_borders.left,
            rect.right() - scaled_borders.right,
            rect.right(),
        ];
        let destination_y: [f32; 4] = [
            rect.top(),
            rect.top() + scaled_borders.top,
            rect.bottom() - scaled_borders.bottom,
            rect.bottom(),
        ];
        let source_x: [f32; 4] = source_axis(self.image.uv.min.x, self.image.uv.max.x, source_size.x, source_borders.left, source_borders.right);
        let source_y: [f32; 4] = source_axis(self.image.uv.min.y, self.image.uv.max.y, source_size.y, source_borders.top, source_borders.bottom);
        let mut shapes: Vec<Shape> = Vec::with_capacity(9);
        for row_index in 0..3 {
            for column_index in 0..3 {
                let destination_cell: Rect = Rect::from_min_max(
                    pos2(destination_x[column_index], destination_y[row_index]),
                    pos2(destination_x[column_index + 1], destination_y[row_index + 1]),
                );
                let source_cell: Rect = Rect::from_min_max(
                    pos2(source_x[column_index], source_y[row_index]),
                    pos2(source_x[column_index + 1], source_y[row_index + 1]),
                );
                let mode: UiNineSliceMode = if row_index == 1 && column_index == 1 {
                    self.center
                } else if row_index == 1 || column_index == 1 {
                    self.edges
                } else {
                    UiNineSliceMode::Stretch
                };
                push_cell(
                    &mut shapes,
                    destination_cell,
                    source_cell,
                    &self.image,
                    tint,
                    mode,
                    self.scale.max(0.0),
                );
            }
        }
        Shape::Vec(shapes)
    }

    fn safe_destination_borders(&self, rect: Rect, source_borders: UiInsets) -> UiInsets {
        let scale: f32 = if self.scale.is_finite() {
            self.scale.max(0.0)
        } else {
            0.0
        };
        let requested: UiInsets = source_borders.scaled(scale);
        let horizontal_scale: f32 = if requested.horizontal() > 0.0 {
            (rect.width() / requested.horizontal()).min(1.0)
        } else {
            1.0
        };
        let vertical_scale: f32 = if requested.vertical() > 0.0 {
            (rect.height() / requested.vertical()).min(1.0)
        } else {
            1.0
        };
        let proportional_scale: f32 = horizontal_scale.min(vertical_scale).min(1.0);
        requested.scaled(proportional_scale)
    }

    fn safe_source_borders(&self, source_size: Vec2) -> UiInsets {
        let result: UiInsets = UiInsets {
            top: nonnegative_finite(self.borders.top).min(source_size.y),
            right: nonnegative_finite(self.borders.right).min(source_size.x),
            bottom: nonnegative_finite(self.borders.bottom).min(source_size.y),
            left: nonnegative_finite(self.borders.left).min(source_size.x),
        };
        let horizontal_scale: f32 = if result.horizontal() > source_size.x {
            source_size.x / result.horizontal()
        } else {
            1.0
        };
        let vertical_scale: f32 = if result.vertical() > source_size.y {
            source_size.y / result.vertical()
        } else {
            1.0
        };
        result.scaled(horizontal_scale.min(vertical_scale))
    }
}

fn source_axis(start: f32, end: f32, source_length: f32, leading: f32, trailing: f32) -> [f32; 4] {
    let source_delta: f32 = end - start;
    let leading_fraction: f32 = leading / source_length;
    let trailing_fraction: f32 = trailing / source_length;
    [
        start,
        start + source_delta * leading_fraction,
        end - source_delta * trailing_fraction,
        end,
    ]
}

fn push_cell(
    shapes: &mut Vec<Shape>,
    destination: Rect,
    source: Rect,
    image: &UiImage,
    tint: Color32,
    mode: UiNineSliceMode,
    scale: f32,
) {
    if !is_positive_size(destination.size()) || !is_nonzero_size(source.size()) {
        return;
    }
    if mode == UiNineSliceMode::Stretch {
        shapes.push(Shape::image(image.texture.id, destination, image.sample_uv(source), tint));
        return;
    }
    let source_size: Vec2 = vec2(
        image.texture.pixel_size.x * source.size().x.abs(),
        image.texture.pixel_size.y * source.size().y.abs(),
    );
    let tile_scale: f32 = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    let tile_size: Vec2 = source_size * tile_scale;
    if !is_positive_size(tile_size) {
        return;
    }
    let requested_tile_count_x: usize = (destination.width() / tile_size.x).ceil() as usize;
    let requested_tile_count_y: usize = (destination.height() / tile_size.y).ceil() as usize;
    let tile_count_x: usize = requested_tile_count_x.min(MAX_TILES_PER_AXIS).max(1);
    let tile_count_y: usize = requested_tile_count_y.min(MAX_TILES_PER_AXIS).max(1);
    let tile_width: f32 = if requested_tile_count_x > MAX_TILES_PER_AXIS {
        destination.width() / tile_count_x as f32
    } else {
        tile_size.x
    };
    let tile_height: f32 = if requested_tile_count_y > MAX_TILES_PER_AXIS {
        destination.height() / tile_count_y as f32
    } else {
        tile_size.y
    };
    let sampled_source: Rect = image.sample_uv(source);
    for tile_y in 0..tile_count_y {
        for tile_x in 0..tile_count_x {
            let tile_min_x: f32 = destination.left() + tile_x as f32 * tile_width;
            let tile_min_y: f32 = destination.top() + tile_y as f32 * tile_height;
            let tile_max_x: f32 = (tile_min_x + tile_width).min(destination.right());
            let tile_max_y: f32 = (tile_min_y + tile_height).min(destination.bottom());
            let tile_destination: Rect = Rect::from_min_max(
                pos2(tile_min_x, tile_min_y),
                pos2(tile_max_x, tile_max_y),
            );
            let tile_fraction_x: f32 = tile_destination.width() / tile_width;
            let tile_fraction_y: f32 = tile_destination.height() / tile_height;
            let tile_source: Rect = cropped_uv(sampled_source, tile_fraction_x, tile_fraction_y);
            shapes.push(Shape::image(image.texture.id, tile_destination, tile_source, tint));
        }
    }
}

fn cropped_uv(source: Rect, fraction_x: f32, fraction_y: f32) -> Rect {
    let fraction_x: f32 = fraction_x.clamp(0.0, 1.0);
    let fraction_y: f32 = fraction_y.clamp(0.0, 1.0);
    Rect::from_min_max(
        pos2(source.min.x, source.min.y),
        pos2(
            source.min.x + (source.max.x - source.min.x) * fraction_x,
            source.min.y + (source.max.y - source.min.y) * fraction_y,
        ),
    )
}

fn is_positive_size(size: Vec2) -> bool {
    size.x.is_finite() && size.y.is_finite() && size.x > 0.0 && size.y > 0.0
}

fn is_nonzero_size(size: Vec2) -> bool {
    size.x.is_finite() && size.y.is_finite() && size.x != 0.0 && size.y != 0.0
}

fn nonnegative_finite(value: f32) -> f32 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}
