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
        Self::from_rgba(accelerator, pixels.as_raw(), width, height, columns, rows)
    }

    /// Uploads a single frame filled with one straight-alpha sRGB color
    pub fn from_color(
        accelerator: &Accelerator,
        red: u8,
        green: u8,
        blue: u8,
        alpha: u8,
    ) -> Result<Self, SpriteSheetError> {
        Self::from_rgba(accelerator, &[red, green, blue, alpha], 1, 1, 1, 1)
    }

    /// Uploads straight-alpha sRGB RGBA8 pixels as a grid of frames
    pub fn from_rgba(
        accelerator: &Accelerator,
        pixels: &[u8],
        width: u32,
        height: u32,
        columns: u32,
        rows: u32,
    ) -> Result<Self, SpriteSheetError> {
        let maximum_dimension: u32 = accelerator.device().limits().max_texture_dimension_2d;
        if width == 0 || height == 0 || columns == 0 || rows == 0
            || width > maximum_dimension || height > maximum_dimension
            || width % columns != 0 || height % rows != 0
            || pixels.len() as u64 != u64::from(width) * u64::from(height) * 4
        { return Err(SpriteSheetError::InvalidDimensions); }
        let mip_level_count: u32 = 32 - width.max(height).leading_zeros();
        let size: wgpu::Extent3d = wgpu::Extent3d { width, height, depth_or_array_layers: 1 };
        let texture: wgpu::Texture = accelerator.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("`SpriteSheet.texture_view`"),
            size,
            mip_level_count,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let mut level: Vec<u8> = pixels.to_vec();
        let mut level_width: u32 = width;
        let mut level_height: u32 = height;
        for mip_level in 0..mip_level_count {
            let mip_size: wgpu::Extent3d = wgpu::Extent3d {
                width: level_width,
                height: level_height,
                depth_or_array_layers: 1,
            };
            accelerator.queue().write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &level,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(level_width * 4),
                    rows_per_image: Some(level_height),
                },
                mip_size,
            );
            if mip_level + 1 < mip_level_count {
                let next_width: u32 = (level_width / 2).max(1);
                let next_height: u32 = (level_height / 2).max(1);
                let mut next: Vec<u8> = Vec::with_capacity((next_width * next_height * 4) as usize);
                for y in 0..next_height {
                    for x in 0..next_width {
                        let x_start: u32 = x * level_width / next_width;
                        let x_end: u32 = ((x + 1) * level_width / next_width).max(x_start + 1);
                        let y_start: u32 = y * level_height / next_height;
                        let y_end: u32 = ((y + 1) * level_height / next_height).max(y_start + 1);
                        let mut sum: [f32; 4] = [0.0; 4];
                        let mut count: f32 = 0.0;
                        for source_y in y_start..y_end {
                            for source_x in x_start..x_end {
                                let index: usize = ((source_y * level_width + source_x) * 4) as usize;
                                for channel in 0..3 {
                                    let value: f32 = f32::from(level[index + channel]) / 255.0;
                                    sum[channel] += if value <= 0.04045 {
                                        value / 12.92
                                    } else {
                                        ((value + 0.055) / 1.055).powf(2.4)
                                    };
                                }
                                sum[3] += f32::from(level[index + 3]) / 255.0;
                                count += 1.0;
                            }
                        }
                        for channel in 0..3 {
                            let value: f32 = sum[channel] / count;
                            let encoded: f32 = if value <= 0.0031308 {
                                value * 12.92
                            } else {
                                1.055 * value.powf(1.0 / 2.4) - 0.055
                            };
                            next.push((encoded * 255.0).round().clamp(0.0, 255.0) as u8);
                        }
                        next.push(((sum[3] / count) * 255.0).round().clamp(0.0, 255.0) as u8);
                    }
                }
                level = next;
                level_width = next_width;
                level_height = next_height;
            }
        }
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
