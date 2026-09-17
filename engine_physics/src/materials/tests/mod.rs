// Copyright Rob Gage 2026

use super::{MaterialForm, MaterialIdentifier};
use crate::materials::{
    Material, MaterialReaction, MaterialReactionProduct, MaterialReactionReactant,
    MaterialReference, MaterialRegistry, MaterialRegistryBuilder, MaterialThermalProperties,
    MaterialThermalTransition,
};
use engine_graphics::{Color, MaterialAppearance};

#[test]
fn test_gas_uses_nonzero_tag_zero_identifiers_without_changing_existing_forms() {
    assert!(MaterialIdentifier::NULL.as_u32() == 0);
    assert!(MaterialIdentifier::NULL.form_checked().is_none());
    assert!(MaterialIdentifier::NULL.index() == 0);
    assert!(MaterialIdentifier::new(MaterialForm::Gas, 0).as_u32() == 0x00000001);
    assert!(MaterialIdentifier::new(MaterialForm::Gas, 1).as_u32() == 0x00000002);
    assert!(
        MaterialIdentifier::new(MaterialForm::Gas, 1).form_checked() == Some(MaterialForm::Gas)
    );
    assert!(MaterialIdentifier::new(MaterialForm::Gas, 1).index() == 1);
    assert!(MaterialIdentifier::new(MaterialForm::Gas, 0x3ffffffe).as_u32() == 0x3fffffff);
    assert!(MaterialIdentifier::new(MaterialForm::Gas, 0x3ffffffe).index() == 0x3ffffffe);
    assert!(MaterialIdentifier::new(MaterialForm::CellularStatic, 0).as_u32() == 0x40000000);
    assert!(MaterialIdentifier::new(MaterialForm::CellularDynamic, 0).as_u32() == 0x80000000);
    assert!(MaterialIdentifier::new(MaterialForm::Fluid, 0).as_u32() == 0xc0000000);
}

#[test]
fn test_gas_round_trips_and_three_form_registry_remains_readable() {
    let mut registry: MaterialRegistry = MaterialRegistry::new();
    let identifier: MaterialIdentifier = registry.register(Material::Gas {
        name: "Test Gas".into(),
        graphics: MaterialAppearance::from_color(Color::new_rgb(1, 2, 3)),
        density: 0.75,
        diffusivity: 0.2,
        extinction: 0.1,
        dissipation: 0.3,
        compressibility: 0.2,
    });
    let mut bytes: Vec<u8> = Vec::new();
    registry.serialize(&mut bytes).unwrap();
    let mut reader: &[u8] = &bytes;
    let loaded: MaterialRegistry = MaterialRegistry::deserialize(&mut reader).unwrap();
    assert!(matches!(loaded.get(identifier), Some(Material::Gas {
        name, density, diffusivity, extinction, dissipation, compressibility, ..
    }) if name == "Test Gas" && *density == 0.75 && *diffusivity == 0.2 &&
        *extinction == 0.1 && *dissipation == 0.3 && *compressibility == 0.2));

    let empty_registry: MaterialRegistry = MaterialRegistry::new();
    let mut old_bytes: Vec<u8> = Vec::new();
    empty_registry.serialize(&mut old_bytes).unwrap();
    old_bytes.truncate(
        old_bytes
            .windows(8)
            .position(|bytes| bytes == b"dwmtmeta")
            .unwrap(),
    );
    let mut old_reader: &[u8] = &old_bytes;
    let old_loaded: MaterialRegistry = MaterialRegistry::deserialize(&mut old_reader).unwrap();
    assert!(old_loaded.material_count() == 0);
}

#[test]
fn test_static_fracture_product_round_trips_for_any_registered_form() {
    let mut registry = MaterialRegistry::new();
    let fluid = registry.register(Material::Fluid {
        name: "Water".into(),
        graphics: MaterialAppearance::from_color(Color::new_rgb(1, 2, 3)),
        pressure_transmission: 0.5,
        friction: 0.5,
        restitution: 0.0,
        rest_density: 1.0,
        artificial_pressure: 0.0,
        xsph_smoothing: 0.0,
        body_push_speed: 1.0,
        density: 1.0,
        viscosity: 0.0,
    });
    let stone = registry.register(Material::CellularStatic {
        name: "Stone".into(),
        graphics: MaterialAppearance::from_color(Color::new_rgb(4, 5, 6)),
        mass: 1.0,
        pressure_ignore_threshold: 1.0,
        default_integrity: 2.0,
        minimum_rigid_body_cell_count: 1,
        debris_material: Some(fluid),
        debris_yield_rate: 1.0,
        pressure_transmission: 0.5,
        friction: 0.5,
        restitution: 0.0,
    });
    let mut bytes = Vec::new();
    registry.serialize(&mut bytes).unwrap();
    assert!(matches!(
        MaterialRegistry::deserialize(&mut bytes.as_slice()).unwrap().get(stone),
        Some(Material::CellularStatic { debris_material: Some(identifier), .. }) if *identifier == fluid
    ));
}

#[test]
fn test_builder_compiles_dense_indices_tags_and_thermal_metadata() {
    let mut builder = MaterialRegistryBuilder::new();
    let static_id = builder.register(Material::CellularStatic {
        name: "s".into(),
        graphics: MaterialAppearance::from_color(Color::new_rgb(1, 1, 1)),
        mass: 1.0,
        pressure_ignore_threshold: 1.0,
        default_integrity: 1.0,
        minimum_rigid_body_cell_count: 1,
        debris_material: None,
        debris_yield_rate: 0.0,
        pressure_transmission: 0.5,
        friction: 0.5,
        restitution: 0.0,
    });
    let dynamic_id = builder.register(Material::CellularDynamic {
        name: "d".into(),
        graphics: MaterialAppearance::from_color(Color::new_rgb(1, 1, 1)),
        mass: 1.0,
        pressure_transmission: 0.5,
        friction: 0.5,
        restitution: 0.0,
    });
    let fluid_id = builder.register(Material::Fluid {
        name: "f".into(),
        graphics: MaterialAppearance::from_color(Color::new_rgb(1, 1, 1)),
        pressure_transmission: 0.5,
        friction: 0.5,
        restitution: 0.0,
        rest_density: 1.0,
        artificial_pressure: 0.0,
        xsph_smoothing: 0.0,
        body_push_speed: 0.0,
        density: 1.0,
        viscosity: 0.0,
    });
    let gas_id = builder.register(Material::Gas {
        name: "g".into(),
        graphics: MaterialAppearance::from_color(Color::new_rgb(1, 1, 1)),
        density: 1.0,
        diffusivity: 0.0,
        extinction: 0.0,
        dissipation: 0.0,
        compressibility: 0.0,
    });
    builder.tag(static_id, "mixed").unwrap();
    builder.tag(fluid_id, "mixed").unwrap();
    builder.tag(gas_id, "mixed").unwrap();
    builder.register_reaction(MaterialReaction {
        reactants: [
            Some(MaterialReactionReactant {
                selector: MaterialReference::Tag("mixed".into()),
                amount: 1.0,
            }),
            None,
        ],
        products: [
            Some(MaterialReactionProduct {
                material: dynamic_id,
                amount: 0.5,
            }),
            None,
        ],
        minimum_temperature: Some(300.0),
        maximum_temperature: Some(600.0),
        priority: 4,
        ..Default::default()
    });
    builder
        .set_thermal(
            static_id,
            MaterialThermalProperties {
                conductivity: 3.5,
                specific_heat_capacity: 2.25,
                default_temperature: Some(315.0),
                cold_transition: Some(MaterialThermalTransition {
                    threshold_temperature: 250.0,
                    target: fluid_id,
                    yield_rate: 0.75,
                    latent_energy: 4.0,
                }),
                hot_transition: Some(MaterialThermalTransition {
                    threshold_temperature: 400.0,
                    target: dynamic_id,
                    yield_rate: 1.0,
                    latent_energy: 1.0,
                }),
                ..Default::default()
            },
        )
        .unwrap();
    let registry = builder.compile().unwrap();
    for identifier in [static_id, dynamic_id, fluid_id, gas_id] {
        assert_eq!(
            registry.identifier_from_dense_index(registry.dense_index(identifier).unwrap()),
            Some(identifier)
        );
    }
    assert_eq!(
        registry.tag_members("mixed").unwrap(),
        &[gas_id, static_id, fluid_id]
    );
    assert_eq!(
        registry
            .thermal_properties(static_id)
            .unwrap()
            .hot_transition
            .as_ref()
            .unwrap()
            .target,
        dynamic_id
    );
    assert_eq!(registry.reactions().len(), 1);
    assert_eq!(registry.reactions()[0].priority, 4);
    assert_eq!(
        registry.reaction_selector_members(),
        &[gas_id, static_id, fluid_id]
    );
    let mut bytes = Vec::new();
    registry.serialize(&mut bytes).unwrap();
    let loaded = MaterialRegistry::deserialize(&mut bytes.as_slice()).unwrap();
    assert_eq!(
        loaded.tag_members("mixed").unwrap(),
        &[gas_id, static_id, fluid_id]
    );
    assert_eq!(
        loaded.thermal_properties(static_id),
        registry.thermal_properties(static_id)
    );
    assert_eq!(loaded.reactions().len(), 1);
    assert_eq!(loaded.reactions()[0].priority, 4);
    assert_eq!(
        loaded.reaction_selector_members(),
        registry.reaction_selector_members()
    );
}

#[test]
fn test_reaction_compiler_rejects_invalid_authoring_and_accepts_pressure_only_rules() {
    let mut builder = MaterialRegistryBuilder::new();
    let id = builder.register(Material::CellularDynamic {
        name: "a".into(),
        graphics: MaterialAppearance::from_color(Color::new_rgb(1, 1, 1)),
        mass: 1.0,
        pressure_transmission: 0.5,
        friction: 0.5,
        restitution: 0.0,
    });
    builder.register_reaction(MaterialReaction {
        minimum_pressure: Some(2.0),
        pressure_output: 1.0,
        ..Default::default()
    });
    let registry = builder.compile().unwrap();
    assert_eq!(registry.reactions().len(), 1);

    let invalid = |reaction: MaterialReaction| {
        let mut builder = MaterialRegistryBuilder::new();
        let known = builder.register(Material::CellularDynamic {
            name: "a".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(1, 1, 1)),
            mass: 1.0,
            pressure_transmission: 0.5,
            friction: 0.5,
            restitution: 0.0,
        });
        builder.register_reaction(reaction);
        (builder.compile(), known)
    };
    assert!(invalid(MaterialReaction::default()).0.is_err());
    assert!(
        invalid(MaterialReaction {
            minimum_temperature: Some(3.0),
            maximum_temperature: Some(2.0),
            thermal_energy: 1.0,
            ..Default::default()
        })
        .0
        .is_err()
    );
    assert!(
        invalid(MaterialReaction {
            minimum_pressure: Some(3.0),
            maximum_pressure: Some(2.0),
            thermal_energy: 1.0,
            ..Default::default()
        })
        .0
        .is_err()
    );
    assert!(
        invalid(MaterialReaction {
            minimum_air: Some(0.9),
            maximum_air: Some(0.1),
            thermal_energy: 1.0,
            ..Default::default()
        })
        .0
        .is_err()
    );
    assert!(
        invalid(MaterialReaction {
            minimum_pressure: Some(1.0),
            thermal_energy: f32::NAN,
            ..Default::default()
        })
        .0
        .is_err()
    );
    assert!(
        invalid(MaterialReaction {
            minimum_pressure: Some(1.0),
            maximum_extent_per_tick: -1.0,
            ..Default::default()
        })
        .0
        .is_err()
    );
    let unknown = MaterialIdentifier::from_u32(0x3fff_ffff);
    assert!(
        invalid(MaterialReaction {
            reactants: [
                Some(MaterialReactionReactant {
                    selector: MaterialReference::Material(unknown),
                    amount: 1.0
                }),
                None
            ],
            thermal_energy: 1.0,
            ..Default::default()
        })
        .0
        .is_err()
    );
    assert!(
        invalid(MaterialReaction {
            reactants: [
                Some(MaterialReactionReactant {
                    selector: MaterialReference::Tag("nope".into()),
                    amount: 1.0
                }),
                None
            ],
            thermal_energy: 1.0,
            ..Default::default()
        })
        .0
        .is_err()
    );
    let _ = id;
}

#[test]
fn test_malformed_metadata_extension_is_rejected() {
    let registry = MaterialRegistry::new();
    let mut bytes = Vec::new();
    registry.serialize(&mut bytes).unwrap();
    let extension = bytes
        .windows(8)
        .position(|value| value == b"dwmtmeta")
        .unwrap();
    bytes[extension + 8] = 3;
    assert!(MaterialRegistry::deserialize(&mut bytes.as_slice()).is_err());
}

#[test]
fn test_invalid_transition_target_in_metadata_is_rejected() {
    let mut builder = MaterialRegistryBuilder::new();
    let source = builder.register(Material::CellularDynamic {
        name: "source".into(),
        graphics: MaterialAppearance::from_color(Color::new_rgb(1, 1, 1)),
        mass: 1.0,
        pressure_transmission: 0.5,
        friction: 0.5,
        restitution: 0.0,
    });
    let target = builder.register(Material::CellularDynamic {
        name: "target".into(),
        graphics: MaterialAppearance::from_color(Color::new_rgb(2, 2, 2)),
        mass: 1.0,
        pressure_transmission: 0.5,
        friction: 0.5,
        restitution: 0.0,
    });
    builder
        .set_thermal(
            source,
            MaterialThermalProperties {
                hot_transition: Some(MaterialThermalTransition {
                    threshold_temperature: 400.0,
                    target,
                    yield_rate: 1.0,
                    latent_energy: 1.0,
                }),
                ..Default::default()
            },
        )
        .unwrap();
    let registry = builder.compile().unwrap();
    let mut bytes = Vec::new();
    registry.serialize(&mut bytes).unwrap();
    let extension = bytes
        .windows(8)
        .position(|value| value == b"dwmtmeta")
        .unwrap();
    bytes[extension + 16 + 32..extension + 16 + 36].copy_from_slice(&0x3fff_ffffu32.to_le_bytes());
    assert!(MaterialRegistry::deserialize(&mut bytes.as_slice()).is_err());
}
