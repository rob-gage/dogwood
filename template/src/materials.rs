// Copyright Rob Gage 2026

use engine::{
    Accelerator,
    MaterialGraphicsCellularStatic,
    SpriteSheet,
    materials::{
        Material,
        MaterialPropertiesCellularStatic,
    },
};
use std::sync::Arc;

/// Returns the material definitions for the template
pub fn definitions(accelerator: &Accelerator) -> Vec<Material> {
    let sprite_sheet: Arc<SpriteSheet> = Arc::new(
        SpriteSheet::from_png(
            accelerator,
            include_bytes!("../assets/sprites/stone.png"),
            1,
            1,
        )
        .unwrap_or_else(|error| panic!("failed to load template stone sprite: {error:?}")),
    );
    vec![Material::CellularStatic {
        name: String::from("stone"),
        properties: MaterialPropertiesCellularStatic {
            graphics: MaterialGraphicsCellularStatic {
                sprite_sheet,
                frame: 0,
                width: 1.0,
                height: 1.0,
            },
        },
    }]
}
