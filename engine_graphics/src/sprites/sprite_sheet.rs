// Copyright Rob Gage 2026

use super::SpriteSheetError;
use engine_host::acceleration::Accelerator;

/// GPU texture divided into a grid of equal frames numbered row-major from the top-left
#[derive(Debug)]
pub struct SpriteSheet {
    /// View of the sRGB RGBA8 texture holding every frame
    texture_view: wgpu::TextureView,
    /// Number of frames along each row
    columns: u32,
    /// Number of frame rows
    rows: u32,
    /// Frame width in pixels
    width: u32,
    /// Frame height in pixels
    height: u32,
}

impl SpriteSheet {

    /// Decodes PNG data of any bit depth or compression and uploads it as a grid of frames
    pub fn from_png(
        accelerator: &Accelerator,
        bytes: &[u8],
        columns: u32,
        rows: u32,
    ) -> Result<Self, SpriteSheetError> {
        Self::from_image(accelerator, bytes, image::ImageFormat::Png, columns, rows)
    }

    /// Decodes uncompressed or run-length encoded TGA data and uploads it as a grid of frames
    pub fn from_tga(
        accelerator: &Accelerator,
        bytes: &[u8],
        columns: u32,
        rows: u32,
    ) -> Result<Self, SpriteSheetError> {
        Self::from_image(accelerator, bytes, image::ImageFormat::Tga, columns, rows)
    }

    /// Decodes image data as straight-alpha sRGB RGBA8 and uploads it to a new texture
    fn from_image(
        accelerator: &Accelerator,
        bytes: &[u8],
        format: image::ImageFormat,
        columns: u32,
        rows: u32,
    ) -> Result<Self, SpriteSheetError> {
        let pixels: image::RgbaImage = image::load_from_memory_with_format(bytes, format)
            .map_err(SpriteSheetError::Decode)?
            .to_rgba8();
        let (width, height): (u32, u32) = pixels.dimensions();
        let maximum_dimension: u32 = accelerator.device().limits().max_texture_dimension_2d;
        if width == 0 || height == 0 || columns == 0 || rows == 0
            || width > maximum_dimension || height > maximum_dimension
            || width % columns != 0 || height % rows != 0
        { return Err(SpriteSheetError::InvalidDimensions); }
        let size: wgpu::Extent3d = wgpu::Extent3d { width, height, depth_or_array_layers: 1 };
        let texture: wgpu::Texture = accelerator.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("`SpriteSheet.texture_view`"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        accelerator.queue().write_texture(
            texture.as_image_copy(),
            pixels.as_raw(),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            size,
        );
        let texture_view: wgpu::TextureView =
            texture.create_view(&wgpu::TextureViewDescriptor::default());
        Ok(Self { texture_view, columns, rows, width: width / columns, height: height / rows })
    }

    /// Returns the view of the texture holding every frame
    pub fn texture_view(&self) -> &wgpu::TextureView { &self.texture_view }

    /// Returns the frame width in pixels
    pub fn width(&self) -> u32 { self.width }

    /// Returns the frame height in pixels
    pub fn height(&self) -> u32 { self.height }

    /// Returns the number of frames in the sheet
    pub fn frame_count(&self) -> u32 { self.columns * self.rows }

    /// Returns the normalized left, top, right, and bottom texture edges of a frame
    pub fn frame_texture_region(&self, frame: u32) -> Option<[f32; 4]> {
        if frame >= self.frame_count() { return None; }
        let column: u32 = frame % self.columns;
        let row: u32 = frame / self.columns;
        Some([
            column as f32 / self.columns as f32,
            row as f32 / self.rows as f32,
            (column + 1) as f32 / self.columns as f32,
            (row + 1) as f32 / self.rows as f32,
        ])
    }

}
