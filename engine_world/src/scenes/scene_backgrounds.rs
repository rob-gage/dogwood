// Copyright Rob Gage 2026

use super::{
    cellular_tiles::CELLULAR_TILE_SIZE,
    chunks::{
        ChunkBackgroundSprite,
        ChunkBackgroundUpload,
        ChunkCoordinates,
        ChunkStreamingState,
        CHUNK_SIZE,
    },
    ScenePosition,
};
use engine_graphics::{
    sprites::SpriteSheet,
    world::scenes::SceneGraphicsParallaxBatch,
};
use engine_host::acceleration::Accelerator;
use std::{
    collections::HashMap,
    sync::Arc,
};

/// Most chunk backgrounds uploaded per stream, keeping any one frame cheap
const SCENE_BACKGROUND_UPLOADS_PER_STREAM: usize = 1;
/// Furthest a chunk's background sprite overhangs its chunk, in tiles
const SCENE_BACKGROUND_OVERHANG: f64 = 32.0;
/// Bytes of one instance in the sprite renderer's layout
const SPRITE_INSTANCE_BYTES: usize = 80;

/// GPU copy of one chunk's background
struct ResidentBackground {
    /// The chunk's image
    image: Option<wgpu::TextureView>,
    /// Instances of the chunk's sprites and their number
    sprites: Option<(wgpu::Buffer, u32)>,
}

/// Uploaded backgrounds of a scene's resident chunks, drawn as static batches so that thousands of
/// sprites cost nothing per frame until their chunks are visible
pub struct SceneBackgrounds {
    /// Sheet of every sprite in the backgrounds
    sprite_sheet: Option<Arc<SpriteSheet>>,
    /// Radiance of backgrounds, which emit no light
    black: Option<SpriteSheet>,
    /// Backgrounds by chunk
    residents: HashMap<ChunkCoordinates, ResidentBackground>,
    /// Images of unloaded chunks, reused so that streaming rarely allocates a texture
    free_images: Vec<wgpu::TextureView>,
    /// The one instance every chunk draws its image with, since images are placed by their batch
    image_instance: Option<wgpu::Buffer>,
}

impl SceneBackgrounds {

    /// Creates an empty set of backgrounds
    pub fn new() -> Self {
        Self {
            sprite_sheet: None,
            black: None,
            residents: HashMap::new(),
            free_images: Vec::new(),
            image_instance: None,
        }
    }

    /// Sets the sheet of background sprites
    pub fn set_sprite_sheet(&mut self, sprite_sheet: Arc<SpriteSheet>) {
        self.sprite_sheet = Some(sprite_sheet);
    }

    /// Uploads a few waiting backgrounds, nearest the camera first, and drops those of unloaded
    /// chunks
    pub fn stream(
        &mut self,
        accelerator: &Accelerator,
        chunk_streaming_state: &mut ChunkStreamingState,
        camera_position: ScenePosition,
    ) {
        let free_images: &mut Vec<wgpu::TextureView> = &mut self.free_images;
        self.residents.retain(|coordinates, resident| {
            let keep: bool = chunk_streaming_state.is_chunk_resident(*coordinates);
            if !keep && let Some(image) = resident.image.take() { free_images.push(image); }
            keep
        });
        let tiles_per_chunk: i64 = CHUNK_SIZE / CELLULAR_TILE_SIZE;
        let nearest: ChunkCoordinates = ChunkCoordinates {
            x: camera_position.tile_x.div_euclid(tiles_per_chunk) as i32,
            y: camera_position.tile_y.div_euclid(tiles_per_chunk) as i32,
        };
        for (coordinates, background) in chunk_streaming_state
            .take_backgrounds(nearest, SCENE_BACKGROUND_UPLOADS_PER_STREAM)
        {
            let resident: ResidentBackground = self.upload(accelerator, &background);
            self.residents.insert(coordinates, resident);
        }
    }

    /// Uploads one background
    fn upload(
        &mut self,
        accelerator: &Accelerator,
        background: &ChunkBackgroundUpload,
    ) -> ResidentBackground {
        let chunk_tiles: f32 = (CHUNK_SIZE / CELLULAR_TILE_SIZE) as f32;
        if self.image_instance.is_none() {
            let bytes: Vec<u8> = instance_bytes(
                [chunk_tiles * 0.5, chunk_tiles * 0.5, chunk_tiles, chunk_tiles],
                0.0,
                [0.0, 0.0, 1.0, 1.0],
                [1.0; 4],
            );
            self.image_instance = Some(instance_buffer(accelerator, &bytes));
        }
        let image: Option<wgpu::TextureView> = background.image.as_ref().map(|image| {
            let view: wgpu::TextureView = self.free_images.pop()
                .unwrap_or_else(|| image_view(accelerator));
            image_write(accelerator, &view, image);
            view
        });
        let sprites: Option<(wgpu::Buffer, u32)> = self.sprite_sheet.as_ref()
            .filter(|_| !background.sprites.is_empty())
            .map(|sheet| {
                let cells_per_tile: f32 = CELLULAR_TILE_SIZE as f32;
                let mut bytes: Vec<u8> =
                    Vec::with_capacity(background.sprites.len() * SPRITE_INSTANCE_BYTES);
                let mut count: u32 = 0;
                for sprite in &background.sprites {
                    let sprite: &ChunkBackgroundSprite = sprite;
                    let Some(frame) = sheet.frame_texture_region(sprite.frame) else { continue; };
                    let [left, top, right, bottom]: [f32; 4] = sprite.crop;
                    let region: [f32; 4] = [
                        frame[0] + (frame[2] - frame[0]) * left,
                        frame[1] + (frame[3] - frame[1]) * top,
                        frame[0] + (frame[2] - frame[0]) * right,
                        frame[1] + (frame[3] - frame[1]) * bottom,
                    ];
                    bytes.extend(instance_bytes(
                        [
                            sprite.x / cells_per_tile,
                            sprite.y / cells_per_tile,
                            sprite.width / cells_per_tile,
                            sprite.height / cells_per_tile,
                        ],
                        sprite.rotation,
                        region,
                        sprite.color,
                    ));
                    count += 1;
                }
                (instance_buffer(accelerator, &bytes), count)
            })
            .filter(|(_, count)| *count > 0);
        if self.black.is_none() {
            self.black = SpriteSheet::from_color(accelerator, 0, 0, 0, 255).ok();
        }
        ResidentBackground { image, sprites }
    }

    /// Returns the batches of the chunks a camera can see, every image before every sprite so that
    /// sprites overhanging into neighboring chunks stay visible
    pub fn batches(
        &self,
        camera_position: ScenePosition,
        visible_width: f32,
        visible_height: f32,
    ) -> Vec<SceneGraphicsParallaxBatch<'_>> {
        let Some(black) = self.black.as_ref() else { return Vec::new(); };
        let tiles_per_chunk: i64 = CHUNK_SIZE / CELLULAR_TILE_SIZE;
        let mut images: Vec<SceneGraphicsParallaxBatch<'_>> = Vec::new();
        let mut sprites: Vec<SceneGraphicsParallaxBatch<'_>> = Vec::new();
        for (coordinates, resident) in &self.residents {
            let (x, y): (f64, f64) = ScenePosition {
                tile_x: i64::from(coordinates.x) * tiles_per_chunk,
                tile_y: i64::from(coordinates.y) * tiles_per_chunk,
                offset_x: 0,
                offset_y: 0,
            }.relative_position(camera_position);
            let chunk: f64 = tiles_per_chunk as f64;
            let half_width: f64 = f64::from(visible_width) * 0.5 + SCENE_BACKGROUND_OVERHANG;
            let half_height: f64 = f64::from(visible_height) * 0.5 + SCENE_BACKGROUND_OVERHANG;
            if x > half_width || x + chunk < -half_width || y > half_height || y + chunk < -half_height {
                continue;
            }
            let origin: [f32; 2] = [x as f32, y as f32];
            if let (Some(view), Some(buffer)) = (&resident.image, &self.image_instance) {
                images.push(SceneGraphicsParallaxBatch {
                    texture: view,
                    radiance_texture: black.texture_view(),
                    instance_buffer: buffer,
                    instance_count: 1,
                    origin,
                });
            }
            if let (Some((buffer, count)), Some(sheet)) = (&resident.sprites, &self.sprite_sheet) {
                sprites.push(SceneGraphicsParallaxBatch {
                    texture: sheet.texture_view(),
                    radiance_texture: black.texture_view(),
                    instance_buffer: buffer,
                    instance_count: *count,
                    origin,
                });
            }
        }
        images.append(&mut sprites);
        images
    }

}

/// Returns one sprite instance in the sprite renderer's layout, emitting no light
fn instance_bytes(
    position_size: [f32; 4],
    rotation: f32,
    region: [f32; 4],
    color: [f32; 4],
) -> Vec<u8> {
    position_size.into_iter()
        .chain([rotation, 0.0])
        .chain(region)
        .chain(color)
        .chain([0.0, 0.0, 1.0, 1.0])
        .chain([0.0, 0.0])
        .flat_map(f32::to_le_bytes)
        .collect()
}

/// Uploads vertex instances
fn instance_buffer(accelerator: &Accelerator, bytes: &[u8]) -> wgpu::Buffer {
    let buffer: wgpu::Buffer = accelerator.device().create_buffer(&wgpu::BufferDescriptor {
        label: Some("`SceneBackgrounds` instances"),
        size: bytes.len() as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    accelerator.queue().write_buffer(&buffer, 0, bytes);
    buffer
}

/// Creates the texture of a chunk's image, without mipmaps since backgrounds are always drawn near
/// one texel per cell
fn image_view(accelerator: &Accelerator) -> wgpu::TextureView {
    let side: u32 = CHUNK_SIZE as u32;
    accelerator.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("`SceneBackgrounds` image"),
        size: wgpu::Extent3d { width: side, height: side, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    }).create_view(&wgpu::TextureViewDescriptor::default())
}

/// Writes prepared bytes into the texture of a chunk's image
fn image_write(accelerator: &Accelerator, view: &wgpu::TextureView, bytes: &[u8]) {
    let side: u32 = CHUNK_SIZE as u32;
    accelerator.queue().write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: view.texture(),
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(side * 4),
            rows_per_image: Some(side),
        },
        wgpu::Extent3d { width: side, height: side, depth_or_array_layers: 1 },
    );
}
