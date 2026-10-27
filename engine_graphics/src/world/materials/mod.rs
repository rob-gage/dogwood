// Copyright Rob Gage 2026

mod material_color_transformation;
mod material_graphics_cellular_dynamic;
mod material_graphics_cellular_static;
mod material_graphics_gas;
mod material_radiance;

pub use material_color_transformation::MaterialColorTransformation;
pub use material_graphics_cellular_dynamic::MaterialGraphicsCellularDynamic;
pub use material_graphics_cellular_static::MaterialGraphicsCellularStatic;
pub use material_graphics_gas::MaterialGraphicsGas;
pub use material_radiance::{
    MATERIAL_RADIANCE_MAXIMUM,
    MATERIAL_RADIANCE_WGSL,
    pack_radiance,
    unpack_radiance,
};
