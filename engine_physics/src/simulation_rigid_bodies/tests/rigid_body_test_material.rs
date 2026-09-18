use engine_graphics::Color;
use engine_graphics::MaterialAppearance;

use crate::materials::Material;
use crate::materials::MaterialIdentifier;
use crate::materials::MaterialRegistry;

pub(super) fn register_test_stone_material(
    materials: &mut MaterialRegistry,
    pressure_ignore_threshold: f32,
    default_integrity: f32,
) -> MaterialIdentifier {
    materials.register(Material::CellularStatic {
        name: "Stone".into(),
        graphics: MaterialAppearance::from_color(Color::new_rgb(90, 90, 90)),
        mass: 1.0,
        pressure_ignore_threshold,
        default_integrity,
        minimum_rigid_body_cell_count: 1,
        debris_material: None,
        debris_yield_rate: 0.0,
        pressure_transmission: 1.0,
        friction: 0.5,
        restitution: 0.0,
    })
}
